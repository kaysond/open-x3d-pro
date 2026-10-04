/* Open X3D Pro pipeline, CONTRACT section 4.1. Doubles throughout: this runs in
 * user mode (UMDF host / test), and floats keep us within +-1 LSB of the Rust
 * reference without a fixed-point error analysis. */
#include "pipeline.h"

#include <string.h>

uint32_t pipeline_crc32(const void *data, size_t len)
{
    const uint8_t *p = (const uint8_t *)data;
    uint32_t crc = 0xFFFFFFFFu;
    while (len--) {
        crc ^= *p++;
        for (int k = 0; k < 8; k++)
            crc = (crc >> 1) ^ (0xEDB88320u & (0u - (crc & 1u)));
    }
    return ~crc;
}

static uint32_t blob_crc(const X3D_CONFIG *cfg)
{
    return pipeline_crc32((const uint8_t *)cfg + 12, sizeof(X3D_CONFIG) - 12);
}

bool pipeline_validate(const uint8_t *blob, size_t len)
{
    X3D_CONFIG cfg;
    if (blob == NULL || len < sizeof(X3D_CONFIG))
        return false;
    memcpy(&cfg, blob, sizeof(cfg));
    return cfg.magic == X3D_CONFIG_MAGIC && cfg.version == X3D_CONFIG_VERSION &&
           cfg.size == X3D_CONFIG_SIZE && cfg.crc32 == blob_crc(&cfg);
}

void pipeline_identity(X3D_CONFIG *out)
{
    memset(out, 0, sizeof(*out));
    out->magic = X3D_CONFIG_MAGIC;
    out->version = (uint16_t)X3D_CONFIG_VERSION;
    out->size = X3D_CONFIG_SIZE;
    for (uint8_t i = 0; i < 4; i++) {
        X3D_AXIS_CONFIG *a = &out->axes[i];
        bool full = i == X3D_AXIS_SLIDER;
        a->cal_min = 0;
        a->cal_center = full ? 0 : 32768;
        a->cal_max = 65535;
        a->mode = (uint8_t)(full ? X3D_MODE_FULL : X3D_MODE_SYMMETRIC);
        a->target = i;
        for (uint32_t k = 0; k <= 16; k++)
            a->lut[k] = (uint16_t)((k * 65535u + 8u) / 16u); /* round(k*65535/16) */
    }
    out->hat_mode = (uint8_t)X3D_HAT_MODE_HAT;
    for (uint8_t i = 0; i < 12; i++)
        out->button_map[i] = (uint8_t)(i + 1);
    out->crc32 = blob_crc(out);
}

static double clamp(double v, double lo, double hi)
{
    return v < lo ? lo : (v > hi ? hi : v);
}

/* Division where den == 0 (degenerate calibration/deadzones) saturates the way
 * num/den -> +-inf would after the clamp that always follows, instead of NaN. */
static double ratio(double num, double den)
{
    if (den != 0.0)
        return num / den;
    return num > 0.0 ? 1.0 : (num < 0.0 ? -1.0 : 0.0);
}

static double lut_lookup(const X3D_AXIS_CONFIG *a, double x)
{
    double pos = clamp(x, 0.0, 1.0) * 16.0;
    int i = (int)pos;
    if (i > 15)
        i = 15;
    double lo = a->lut[i], hi = a->lut[i + 1];
    return lo + (hi - lo) * (pos - (double)i);
}

static uint16_t to_u16(double v)
{
    return (uint16_t)(clamp(v, 0.0, 65535.0) + 0.5); /* round half up; v >= 0 here */
}

static uint16_t process_axis(const X3D_AXIS_CONFIG *a, unsigned raw16)
{
    double r = raw16, min = a->cal_min, ctr = a->cal_center, max = a->cal_max;
    double dzc = a->dz_center / 1000.0, dzmin = a->dz_min / 1000.0, dzmax = a->dz_max / 1000.0;

    if (a->mode == X3D_MODE_FULL) {
        double v = clamp(ratio(r - min, max - min), 0.0, 1.0);
        if (a->invert)
            v = 1.0 - v;
        v = clamp(ratio(v - dzmin, 1.0 - dzmin - dzmax), 0.0, 1.0);
        return to_u16(lut_lookup(a, v));
    }

    double v = r <= ctr ? -ratio(ctr - r, ctr - min) : ratio(r - ctr, max - ctr);
    v = clamp(v, -1.0, 1.0);
    if (a->invert)
        v = -v;
    double m = v < 0.0 ? -v : v;
    double dz_end = v < 0.0 ? dzmin : dzmax;
    /* inside the centre deadzone sign(v) counts as 0, so a LUT with lut[0] > 0
     * still rests at exactly 32768 (same as the Rust reference) */
    if (m <= dzc)
        return 32768;
    m = clamp(ratio(m - dzc, 1.0 - dzc - dz_end), 0.0, 1.0);
    return to_u16(32768.0 + (v < 0.0 ? -1.0 : 1.0) * lut_lookup(a, m) / 2.0);
}

/* Hat 0..7 clockwise from north -> output buttons 13..16 (U, R, D, L) as bit masks. */
static const uint32_t hat_buttons[8] = {
    1u << 12,              /* N */
    (1u << 12) | (1u << 13), /* NE */
    1u << 13,              /* E */
    (1u << 13) | (1u << 14), /* SE */
    1u << 14,              /* S */
    (1u << 14) | (1u << 15), /* SW */
    1u << 15,              /* W */
    (1u << 15) | (1u << 12), /* NW */
};

void pipeline_process(const X3D_CONFIG *cfg, const uint8_t raw[X3D_RAW_SIZE], X3D_JOY_REPORT *out)
{
    unsigned x = raw[0] | ((raw[1] & 0x03u) << 8);
    unsigned y = (raw[1] >> 2) | ((raw[2] & 0x0Fu) << 6);
    unsigned hat = raw[2] >> 4;
    unsigned pressed = raw[4] | ((raw[6] & 0x0Fu) << 8);
    unsigned phys16[4];
    uint16_t axes[4];
    uint32_t buttons = 0;

    /* assigned, not brace-initialised: MSVC C warns (C4204) on non-constant initialisers */
    phys16[X3D_AXIS_X] = x << 6;
    phys16[X3D_AXIS_Y] = y << 6;
    phys16[X3D_AXIS_RZ] = (unsigned)raw[3] << 8;
    phys16[X3D_AXIS_SLIDER] = (unsigned)raw[5] << 8;
    for (int i = 0; i < 4; i++)
        axes[i] = (uint16_t)(cfg->axes[i].mode == X3D_MODE_FULL ? 0 : 32768);
    /* ascending order: when two physical axes share a target, the higher index wins */
    for (int i = 0; i < 4; i++) {
        uint8_t t = cfg->axes[i].target;
        if (t < 4)
            axes[t] = process_axis(&cfg->axes[i], phys16[i]);
    }

    bool shift = cfg->shift_button >= 1 && cfg->shift_button <= 12 &&
                 (pressed & (1u << (cfg->shift_button - 1))) != 0;
    const uint8_t *map = shift ? cfg->button_map_shift : cfg->button_map;
    for (int i = 0; i < 12; i++) {
        if ((pressed & (1u << i)) && map[i] >= 1 && map[i] <= 32)
            buttons |= 1u << (map[i] - 1);
    }

    if (hat > 7)
        hat = X3D_HAT_NULL;
    if (hat != X3D_HAT_NULL && cfg->hat_mode != X3D_HAT_MODE_HAT)
        buttons |= hat_buttons[hat];
    if (cfg->hat_mode == X3D_HAT_MODE_BUTTONS)
        hat = X3D_HAT_NULL;

    out->id = (uint8_t)X3D_REPORT_ID_JOY;
    out->x = axes[X3D_AXIS_X];
    out->y = axes[X3D_AXIS_Y];
    out->rz = axes[X3D_AXIS_RZ];
    out->slider = axes[X3D_AXIS_SLIDER];
    out->hat = (uint8_t)hat;
    out->buttons = buttons;
}
