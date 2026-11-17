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

static bool axis_valid(const X3D_AXIS_CONFIG *a)
{
    unsigned c = a->dz_center, lo = a->dz_min, hi = a->dz_max;
    if (a->invert > 1 || (a->target > X3D_AXIS_SLIDER && a->target != X3D_TARGET_NONE))
        return false;
    if (!(a->cal_min < a->cal_center && a->cal_center < a->cal_max))
        return false;
    for (int k = 0; k <= 16; k++) {
        if (a->lut[k] > X3D_OUT_MAX)
            return false;
    }
    return c <= 500 && lo <= 500 && hi <= 500 && c + lo < 1000 && c + hi < 1000 && lo + hi < 1000;
}

bool pipeline_validate(const uint8_t *blob, size_t len)
{
    X3D_CONFIG cfg;
    if (blob == NULL || len < sizeof(X3D_CONFIG))
        return false;
    memcpy(&cfg, blob, sizeof(cfg));
    if (cfg.magic != X3D_CONFIG_MAGIC || cfg.version != X3D_CONFIG_VERSION ||
        cfg.size != X3D_CONFIG_SIZE || cfg.crc32 != blob_crc(&cfg))
        return false;
    for (int i = 0; i < 4; i++) {
        if (!axis_valid(&cfg.axes[i]))
            return false;
    }
    if (cfg.hat_mode > X3D_HAT_MODE_BOTH || cfg.shift_button > 12)
        return false;
    for (int i = 0; i < 12; i++) {
        if (cfg.button_map[i] > 32 || cfg.button_map_shift[i] > 32)
            return false;
    }
    return true;
}

void pipeline_identity(X3D_CONFIG *out)
{
    memset(out, 0, sizeof(*out));
    out->magic = X3D_CONFIG_MAGIC;
    out->version = (uint16_t)X3D_CONFIG_VERSION;
    out->size = X3D_CONFIG_SIZE;
    for (uint8_t i = 0; i < 4; i++) {
        X3D_AXIS_CONFIG *a = &out->axes[i];
        a->cal_min = 0;
        a->cal_center = 32768;
        a->cal_max = 65535;
        a->target = i;
        for (uint32_t k = 0; k <= 16; k++)
            a->lut[k] = (uint16_t)((k * X3D_OUT_MAX + 8u) / 16u); /* round(k*32767/16) */
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

static double lut_lookup(const X3D_AXIS_CONFIG *a, double x)
{
    double pos = clamp(x, 0.0, 1.0) * 16.0;
    int i = (int)pos;
    if (i > 15)
        i = 15;
    double lo = a->lut[i], hi = a->lut[i + 1];
    return lo + (hi - lo) * (pos - (double)i);
}

/* No denominator below can be zero for a config that passed pipeline_validate. */
static int16_t process_axis(const X3D_AXIS_CONFIG *a, unsigned raw16)
{
    double r = raw16, min = a->cal_min, ctr = a->cal_center, max = a->cal_max;
    double dzc = a->dz_center / 1000.0, dzmin = a->dz_min / 1000.0, dzmax = a->dz_max / 1000.0;

    double v = r <= ctr ? -(ctr - r) / (ctr - min) : (r - ctr) / (max - ctr);
    v = clamp(v, -1.0, 1.0);
    if (a->invert)
        v = -v;
    double m = v < 0.0 ? -v : v;
    double dz_end = v < 0.0 ? dzmin : dzmax;
    /* inside the centre deadzone sign(v) counts as 0, so a LUT with lut[0] > 0
     * still rests at exactly 0 (same as the Rust reference) */
    if (m <= dzc)
        return 0;
    m = clamp((m - dzc) / (1.0 - dzc - dz_end), 0.0, 1.0);
    double out = lut_lookup(a, m); /* 0..X3D_OUT_MAX */
    /* round half away from zero, as Rust's f64::round */
    return (int16_t)(v < 0.0 ? -(out + 0.5) : out + 0.5);
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
    int16_t axes[4] = {0, 0, 0, 0};
    uint32_t buttons = 0;

    /* assigned, not brace-initialised: MSVC C warns (C4204) on non-constant initialisers */
    phys16[X3D_AXIS_X] = x << 6;
    phys16[X3D_AXIS_Y] = y << 6;
    phys16[X3D_AXIS_RZ] = (unsigned)raw[3] << 8;
    phys16[X3D_AXIS_SLIDER] = (unsigned)raw[5] << 8;
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
