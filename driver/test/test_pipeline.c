/* Host test for driver/pipeline: runs every case of a vectors.json file
 * (CONTRACT 4.3) through pipeline_process. Axes +-1, hat/buttons exact. */
#include "pipeline.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ---- minimal JSON scanner for the fixed vectors.json shape ---- */

static const char *p;  /* cursor into the file */
static int parse_ok = 1;

static void ws(void)
{
    while (*p && isspace((unsigned char)*p))
        p++;
}

static int eat(char c)
{
    ws();
    if (*p == c) {
        p++;
        return 1;
    }
    return 0;
}

static void expect(char c)
{
    if (!eat(c)) {
        if (parse_ok)
            fprintf(stderr, "parse error: expected '%c' near \"%.20s\"\n", c, p);
        parse_ok = 0;
    }
}

/* Reads a string without escapes (none occur in vectors.json) into out. */
static void str(char *out, size_t cap)
{
    size_t n = 0;
    expect('"');
    while (parse_ok && *p && *p != '"') {
        if (*p == '\\' && p[1])
            p++;
        if (n + 1 < cap)
            out[n++] = *p;
        p++;
    }
    out[n] = 0;
    expect('"');
}

static unsigned long long num(void)
{
    char *end;
    unsigned long long v;
    ws();
    v = strtoull(p, &end, 10);
    if (end == p) {
        fprintf(stderr, "parse error: expected number near \"%.20s\"\n", p);
        parse_ok = 0;
    }
    p = end;
    return v;
}

static void skip(void)
{
    char tmp[2];
    ws();
    if (*p == '"') {
        str(tmp, sizeof(tmp));
    } else if (*p == '{' || *p == '[') {
        char close = *p == '{' ? '}' : ']';
        p++;
        if (eat(close))
            return;
        do {
            if (close == '}') {
                str(tmp, sizeof(tmp));
                expect(':');
            }
            skip();
        } while (parse_ok && eat(','));
        expect(close);
    } else {
        while (*p && !strchr(",]} \t\r\n", *p))
            p++; /* number, true, false, null */
    }
}

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

static void parse_case(test_case *c)
{
    char key[16], buf[64];
    memset(c, 0, sizeof(*c));
    expect('{');
    do {
        str(key, sizeof(key));
        expect(':');
        if (!strcmp(key, "raw")) {
            str(buf, sizeof(buf));
            if (hex(buf, c->raw, sizeof(c->raw)) != X3D_RAW_SIZE) {
                fprintf(stderr, "bad raw \"%s\"\n", buf);
                parse_ok = 0;
            }
        } else if (!strcmp(key, "x")) c->x = num();
        else if (!strcmp(key, "y")) c->y = num();
        else if (!strcmp(key, "rz")) c->rz = num();
        else if (!strcmp(key, "slider")) c->slider = num();
        else if (!strcmp(key, "hat")) c->hat = num();
        else if (!strcmp(key, "buttons")) c->buttons = num();
        else skip();
    } while (parse_ok && eat(','));
    expect('}');
}

static int run_file(const char *path)
{
    static char blob_hex[1024];
    char name[128], key[16];
    int vectors = 0;
    FILE *f = fopen(path, "rb");
    if (!f) {
        perror(path);
        return 1;
    }
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    char *text = malloc((size_t)size + 1);
    if (!text || fread(text, 1, (size_t)size, f) != (size_t)size) {
        fprintf(stderr, "cannot read %s\n", path);
        fclose(f);
        free(text);
        return 1;
    }
    text[size] = 0;
    fclose(f);
    p = text;

    expect('[');
    while (parse_ok && !eat(']')) {
        test_case *cases = NULL;
        size_t ncases = 0;
        uint8_t blob[256];
        size_t blob_len = 0;
        strcpy(name, "?");
        expect('{');
        do {
            str(key, sizeof(key));
            expect(':');
            if (!strcmp(key, "name")) {
                str(name, sizeof(name));
            } else if (!strcmp(key, "blob")) {
                str(blob_hex, sizeof(blob_hex));
                blob_len = hex(blob_hex, blob, sizeof(blob));
            } else if (!strcmp(key, "cases")) {
                expect('[');
                if (!eat(']')) {
                    do {
                        test_case *grown = realloc(cases, (ncases + 1) * sizeof(*cases));
                        if (!grown) {
                            parse_ok = 0;
                            break;
                        }
                        cases = grown;
                        parse_case(&cases[ncases++]);
                    } while (parse_ok && eat(','));
                    expect(']');
                }
            } else {
                skip();
            }
        } while (parse_ok && eat(','));
        expect('}');
        eat(',');

        if (parse_ok) {
            X3D_CONFIG cfg;
            vectors++;
            if (!pipeline_validate(blob, blob_len)) {
                printf("FAIL %s: blob does not validate (len %zu)\n", name, blob_len);
                failures++;
            } else {
                memcpy(&cfg, blob, sizeof(cfg));
                if (!strcmp(name, "identity")) {
                    X3D_CONFIG id;
                    pipeline_identity(&id);
                    if (memcmp(&id, &cfg, sizeof(cfg))) {
                        printf("FAIL identity: blob differs from pipeline_identity()\n");
                        failures++;
                    }
                }
                for (size_t i = 0; i < ncases; i++)
                    run_case(name, (int)i, &cfg, &cases[i]);
            }
        }
        free(cases);
    }
    free(text);
    if (!parse_ok)
        return 1;
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
    return bad;
}

int main(int argc, char **argv)
{
    if (argc != 2) {
        fprintf(stderr, "usage: %s vectors.json\n", argv[0]);
        return 2;
    }
    if (self_checks())
        return 1;
    return run_file(argv[1]);
}
