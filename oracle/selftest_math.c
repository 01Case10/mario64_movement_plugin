/*
 * oracle/selftest_math.c
 *
 * Sanity check that the compiled decomp math_util.o links and that atan2s
 * behaves as expected. Links against math_util.o only.
 *
 * NOTE: atan2s is declared here with its true decomp signature
 * (s16 atan2s(f32 y, f32 x), see src/engine/math_util.h). Declaring it with
 * s16 parameters would be an ABI mismatch on x86-64 (integer arguments are
 * passed in general-purpose registers, float arguments in XMM registers),
 * so the declaration below must match the definition.
 */
#include <stdio.h>

extern short atan2s(float y, float x);

/*
 * Link shims: math_util.o references three symbols it does not define.
 * atan2s never touches them (they are only used by the mtx/spline code in
 * the same object), so minimal local definitions are enough to satisfy the
 * linker for this selftest. Signatures match the decomp headers
 * (src/engine/surface_collision.h, src/engine/graph_node.h, include/PR/gu.h).
 */
float gVec3fZero[3];

float find_floor(float x, float y, float z, void **pfloor) {
    (void) x;
    (void) y;
    (void) z;
    if (pfloor != 0) {
        *pfloor = 0;
    }
    return 0.0f;
}

void guMtxF2L(float mf[4][4], void *m) {
    (void) mf;
    (void) m;
}

struct Case {
    float y;
    float x;
    short want;
};

static const struct Case kCases[] = {
    {   1.0f,  0.0f,     0 },
    {   0.0f,  1.0f, 16384 },
    { -64.0f,  0.0f,-32768 },
    {   0.0f, 64.0f, 16384 },
    {   0.0f,  0.0f,     0 },
    {  30.0f, 40.0f,  9672 },
};

int main(void) {
    unsigned i;
    for (i = 0; i < sizeof(kCases) / sizeof(kCases[0]); i++) {
        short got = atan2s(kCases[i].y, kCases[i].x);
        if (got != kCases[i].want) {
            printf("atan2s selftest: FAIL case %u: atan2s(%g, %g) = %d, want %d\n",
                   i, kCases[i].y, kCases[i].x, got, kCases[i].want);
            return 1;
        }
    }
    printf("atan2s selftest: OK\n");
    return 0;
}
