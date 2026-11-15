/* Host test for driver/pipeline: runs every case of a vectors.txt file
 * (CONTRACT 4.3) through pipeline_process. Axes +-1, hat/buttons exact. */
#include "pipeline.h"

#include <stdio.h>
#include <string.h>

static size_t hex(const char *s, uint8_t *out, size_t cap)
{
    size_t n = 0;
    while (s[0] && s[1] && n < cap) {
        unsigned b;
        if (sscanf(s, "%2x", &b) != 1)
            break;
        out[n++] = (uint8_t)b;
        s += 2;
    }
    return n;
}

/* ---- test ---- */

typedef struct {
    uint8_t raw[X3D_RAW_SIZE];
    unsigned long long x, y, rz, slider, hat, buttons;
} test_case;

static int failures, total;

static int near(unsigned got, unsigned long long want)
{
    return (unsigned long long)got + 1 >= want && got <= want + 1;
}

static void run_case(const char *name, int idx, const X3D_CONFIG *cfg, const test_case *c)
{
    X3D_JOY_REPORT r;
    pipeline_process(cfg, c->raw, &r);
    total++;
    if (r.id == 1 && near(r.x, c->x) && near(r.y, c->y) && near(r.rz, c->rz) && near(r.slider, c->slider) &&
        r.hat == c->hat && r.buttons == c->buttons)
        return;
    failures++;
    printf("FAIL %s[%d]: got x=%u y=%u rz=%u slider=%u hat=%u buttons=0x%08X\n", name, idx, r.x, r.y, r.rz,
           r.slider, r.hat, (unsigned)r.buttons);
    printf("           want x=%llu y=%llu rz=%llu slider=%llu hat=%llu buttons=0x%08llX\n", c->x, c->y, c->rz,
           c->slider, c->hat, c->buttons);
}

/* Lines: "V <name> <blob hex>" starts a vector, "C <raw hex> <x> <y> <rz> <slider> <hat> <buttons>"
 * is one case of the vector above it. */
static int run_file(const char *path)
{
    char line[1024], name[128] = "?", hexbuf[512];
    X3D_CONFIG cfg;
    int have_cfg = 0, idx = 0, vectors = 0;
    FILE *f = fopen(path, "r");
    if (!f) {
        perror(path);
        return 1;
    }
    while (fgets(line, sizeof(line), f)) {
        test_case c;
        if (sscanf(line, "V %127s %511s", name, hexbuf) == 2) {
            uint8_t blob[256];
            size_t blob_len = hex(hexbuf, blob, sizeof(blob));
            vectors++;
            idx = 0;
            have_cfg = pipeline_validate(blob, blob_len);
            if (!have_cfg) {
                printf("FAIL %s: blob does not validate (len %zu)\n", name, blob_len);
                failures++;
                continue;
            }
            memcpy(&cfg, blob, sizeof(cfg));
            if (!strcmp(name, "identity")) {
                X3D_CONFIG id;
                pipeline_identity(&id);
                if (memcmp(&id, &cfg, sizeof(cfg))) {
                    printf("FAIL identity: blob differs from pipeline_identity()\n");
                    failures++;
                }
            }
        } else if (sscanf(line, "C %14s %llu %llu %llu %llu %llu %llu", hexbuf, &c.x, &c.y, &c.rz, &c.slider, &c.hat,
                          &c.buttons) == 7 &&
                   hex(hexbuf, c.raw, sizeof(c.raw)) == X3D_RAW_SIZE) {
            if (have_cfg)
                run_case(name, idx++, &cfg, &c);
        } else if (line[0] != '\n' && line[0] != '\r') {
            fprintf(stderr, "%s: bad line: %s", path, line);
            fclose(f);
            return 1;
        }
    }
    fclose(f);
    printf("%s: %d vectors, %d cases, %d failures\n", path, vectors, total, failures);
    return failures != 0 || total == 0;
}

static int self_checks(void)
{
    X3D_CONFIG id;
    uint8_t bytes[sizeof(X3D_CONFIG)];
    int bad = 0;

    if (pipeline_crc32("123456789", 9) != 0xCBF43926u) {
        printf("FAIL crc32 check value\n");
        bad = 1;
    }
    pipeline_identity(&id);
    memcpy(bytes, &id, sizeof(bytes));
    if (!pipeline_validate(bytes, sizeof(bytes))) {
        printf("FAIL identity blob does not validate\n");
        bad = 1;
    }
    if (pipeline_validate(bytes, sizeof(bytes) - 1)) {
        printf("FAIL short blob validates\n");
        bad = 1;
    }
    bytes[100] ^= 1;
    if (pipeline_validate(bytes, sizeof(bytes))) {
        printf("FAIL corrupted blob validates\n");
        bad = 1;
    }
    /* CRC-valid but out of range: each must be rejected. */
    for (int k = 0; k < 6; k++) {
        X3D_CONFIG c;
        pipeline_identity(&c);
        switch (k) {
        case 0: c.axes[0].cal_center = c.axes[0].cal_max; break;
        case 1: c.axes[2].dz_center = 500; c.axes[2].dz_max = 500; break;
        case 2: c.axes[1].mode = 2; break;
        case 3: c.axes[3].target = 4; break;
        case 4: c.hat_mode = 3; break;
        default: c.button_map_shift[5] = 33; break;
        }
        c.crc32 = pipeline_crc32((const uint8_t *)&c + 12, sizeof(c) - 12);
        memcpy(bytes, &c, sizeof(bytes));
        if (pipeline_validate(bytes, sizeof(bytes))) {
            printf("FAIL out-of-range blob %d validates\n", k);
            bad = 1;
        }
    }
    return bad;
}

int main(int argc, char **argv)
{
    if (argc != 2) {
        fprintf(stderr, "usage: %s vectors.txt\n", argv[0]);
        return 2;
    }
    if (self_checks())
        return 1;
    return run_file(argv[1]);
}
