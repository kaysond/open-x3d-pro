/* Open X3D Pro: axis/button pipeline shared by the UMDF driver and the host test.
 * Pure C, no WDF/Windows headers. Spec: docs/CONTRACT.md section 4. */
#ifndef OPENX3D_PIPELINE_H
#define OPENX3D_PIPELINE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define X3D_CONFIG_MAGIC   0x50443358u /* "X3DP" little-endian */
#define X3D_CONFIG_VERSION 1u
#define X3D_RAW_SIZE       7u          /* device input report, no report ID */

#define X3D_REPORT_ID_JOY    1u
#define X3D_REPORT_ID_VENDOR 2u
#define X3D_REPORT_ID_CONFIG 3u
#define X3D_REPORT_ID_INFO   4u

#define X3D_AXIS_X      0u
#define X3D_AXIS_Y      1u
#define X3D_AXIS_RZ     2u
#define X3D_AXIS_SLIDER 3u
#define X3D_TARGET_NONE 0xFFu
#define X3D_MODE_SYMMETRIC 0u
#define X3D_MODE_FULL      1u
#define X3D_HAT_MODE_HAT     0u
#define X3D_HAT_MODE_BUTTONS 1u
#define X3D_HAT_MODE_BOTH    2u
#define X3D_HAT_NULL 8u

#pragma pack(push, 1)
typedef struct X3D_AXIS_CONFIG {
    uint16_t cal_min, cal_center, cal_max;
    uint16_t dz_center, dz_min, dz_max; /* permille, 0..500 */
    uint8_t invert;                     /* 0/1 */
    uint8_t mode;                       /* X3D_MODE_* */
    uint8_t target;                     /* X3D_AXIS_* or X3D_TARGET_NONE */
    uint8_t reserved;
    uint16_t lut[17];                   /* output for input i/16 */
} X3D_AXIS_CONFIG;

/* Config blob, little-endian (CONTRACT 4.2). */
typedef struct X3D_CONFIG {
    uint32_t magic;
    uint16_t version;
    uint16_t size;
    uint32_t crc32;                     /* IEEE, over bytes 12..239 */
    X3D_AXIS_CONFIG axes[4];            /* X, Y, Rz, Slider */
    uint8_t hat_mode;                   /* X3D_HAT_MODE_* */
    uint8_t shift_button;               /* 0 none, 1..12 */
    uint8_t button_map[12];             /* 0 none, 1..32 */
    uint8_t button_map_shift[12];
    uint16_t reserved;
} X3D_CONFIG;

/* Input report ID 1 as published in the HID descriptor (14 bytes). */
typedef struct X3D_JOY_REPORT {
    uint8_t id;
    uint16_t x, y, rz, slider;
    uint8_t hat;                        /* low nibble: 0..7, 8 = centred; high nibble: pad */
    uint32_t buttons;                   /* bit n = button n+1 */
} X3D_JOY_REPORT;
#pragma pack(pop)

/* C89-style static asserts: MSVC's default C mode lacks _Static_assert. */
typedef char x3d_axis_config_is_50_bytes[sizeof(X3D_AXIS_CONFIG) == 50 ? 1 : -1];
typedef char x3d_config_is_240_bytes[sizeof(X3D_CONFIG) == 240 ? 1 : -1];
typedef char x3d_joy_report_is_14_bytes[sizeof(X3D_JOY_REPORT) == 14 ? 1 : -1];

#define X3D_CONFIG_SIZE ((uint16_t)sizeof(X3D_CONFIG))

uint32_t pipeline_crc32(const void *data, size_t len);
/* Magic, version, size and CRC; len may exceed 240 (zero-padded feature report). */
bool pipeline_validate(const uint8_t *blob, size_t len);
void pipeline_identity(X3D_CONFIG *out);
void pipeline_process(const X3D_CONFIG *cfg, const uint8_t raw[X3D_RAW_SIZE], X3D_JOY_REPORT *out);

#endif
