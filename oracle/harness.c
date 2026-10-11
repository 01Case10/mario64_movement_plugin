/*
 * oracle/harness.c -- Phase 1 native oracle harness.
 *
 * Runs the decomp's real Mario movement code one frame at a time from a
 * line-based scenario file and dumps MarioState fields as CSV.
 *
 * Compiled sources (see build.sh): src/engine/{math_util,surface_collision,
 * surface_load,graph_node}.c and src/game/{mario,mario_step,
 * mario_actions_stationary,mario_actions_moving,mario_actions_airborne,
 * mario_actions_submerged,mario_actions_automatic}.c, plus stubs.c for the
 * ~80 symbols those files need that are not compiled (sound, camera hooks,
 * objects/interactions, save file, globals, ...).
 *
 * Usage: oracle --in <file.oin> --out <file.csv>
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>
#include <math.h>
#include <ctype.h>

/* The decomp targets the N64: whenever TARGET_N64 is defined (even as 0),
 * include/PR/ultratypes.h does `typedef u32 size_t` (plus s32 ssize_t, u32
 * uintptr_t, s32 intptr_t, s32 ptrdiff_t), and include/PR/os_libc.h
 * redeclares bcopy/bzero with that size_t. All of those clash with the host
 * libc headers included above, so rename them away for this TU. The linked
 * decomp objects were compiled with the same -DTARGET_N64=0, so the
 * u32-based spellings are ABI-identical (notably struct Object's
 * `uintptr_t bhvStack[8]` and the IS_64_BIT=0 object-field layout). */
#define size_t decomp_size_t
#define ssize_t decomp_ssize_t
#define uintptr_t decomp_uintptr_t
#define intptr_t decomp_intptr_t
#define ptrdiff_t decomp_ptrdiff_t
#define bcopy decomp_bcopy
#define bzero decomp_bzero

/* Decomp headers. Compiled with -I<decomp>/include -I<decomp>/src -I<decomp>. */
#include "sm64.h"
#include "surface_terrains.h"
#include "macros.h"
#include "game/area.h"
#include "game/camera.h"
#include "game/game_init.h"
#include "game/level_update.h"
#include "game/mario.h"
#include "game/mario_misc.h"
#include "game/memory.h"
#include "game/object_list_processor.h"
#include "engine/graph_node.h"
#include "engine/math_util.h"
#include "engine/surface_load.h"

/* Defined by stubs.c: id of the animation most recently requested through
 * load_patchable_table (-1 until the first request). */
extern s32 gOracleLastAnimId;

/* ------------------------------------------------------------------ */
/* adjust_analog_stick: copied VERBATIM from decomp src/game/game_init.c
 * (it is a single self-contained function; game_init.c itself is NOT
 * compiled because it drags in the whole OS layer).                     */
/* ------------------------------------------------------------------ */

/**
 * Take the updated controller struct and calculate the new x, y, and distance floats.
 */
void adjust_analog_stick(struct Controller *controller) {
    UNUSED u8 filler[8];

    // Reset the controller's x and y floats.
    controller->stickX = 0;
    controller->stickY = 0;

    // Modulate the rawStickX and rawStickY to be the new f32 values by adding/subtracting 6.
    if (controller->rawStickX <= -8) {
        controller->stickX = controller->rawStickX + 6;
    }

    if (controller->rawStickX >= 8) {
        controller->stickX = controller->rawStickX - 6;
    }

    if (controller->rawStickY <= -8) {
        controller->stickY = controller->rawStickY + 6;
    }

    if (controller->rawStickY >= 8) {
        controller->stickY = controller->rawStickY - 6;
    }

    // Calculate f32 magnitude from the center by vector length.
    controller->stickMag =
        sqrtf(controller->stickX * controller->stickX + controller->stickY * controller->stickY);

    // Magnitude cannot exceed 64.0f: if it does, modify the values
    // appropriately to flatten the values down to the allowed maximum value.
    if (controller->stickMag > 64) {
        controller->stickX *= 64 / controller->stickMag;
        controller->stickY *= 64 / controller->stickMag;
        controller->stickMag = 64;
    }
}

/* ------------------------------------------------------------------ */
/* Scenario input                                                      */
/* ------------------------------------------------------------------ */

#define MAX_VERTS 4096
#define MAX_TRIS  8192
/* 1 (COL_INIT) + 1 (VERTEX_INIT) + 3V + 1 (TRI_STOP) + 5T (worst case:
 * one COL_TRI_INIT(type,1) group per tri) + 1 (COL_END), plus slack. */
#define TERRAIN_CAP (8 + 1 + 1 + 3 * MAX_VERTS + 1 + 5 * MAX_TRIS + 1)

struct FrameInput {
    s16 sx;
    s16 sy;
    u16 buttons;
    double camyaw_deg;
};

static struct {
    int have_version;
    int terrain;
    int have_terrain;
    int nverts;
    s16 verts[MAX_VERTS][3];
    int have_verts;
    int ntris;
    s16 tritype[MAX_TRIS];
    s16 triidx[MAX_TRIS][3];
    int have_tris;
    int water_none;
    s16 water_level;
    int have_water;
    double start_pos[3];
    double start_yaw_deg;
    u32 start_action;
    int have_start;
    double poke_forward_vel;
    int have_poke_forward_vel;
    int nframes;
    struct FrameInput *frames;
    int have_frames;
} sScen;

static long sLineNo;
static const char *sInputPath;

static void die(const char *fmt, ...) {
    va_list ap;
    fprintf(stderr, "oracle: %s:%ld: error: ",
            sInputPath != NULL ? sInputPath : "<input>", sLineNo);
    va_start(ap, fmt);
    vfprintf(stderr, fmt, ap);
    va_end(ap);
    fputc('\n', stderr);
    exit(1);
}

static long parse_long(const char *s) {
    char *end;
    long v = strtol(s, &end, 10);
    if (end == s || *end != '\0') {
        die("bad integer '%s'", s);
    }
    return v;
}

static unsigned long parse_base0(const char *s) {
    char *end;
    unsigned long v = strtoul(s, &end, 0);
    if (end == s || *end != '\0') {
        die("bad number '%s'", s);
    }
    return v;
}

static double parse_double(const char *s) {
    char *end;
    double v = strtod(s, &end);
    if (end == s || *end != '\0') {
        die("bad float '%s'", s);
    }
    return v;
}

static s16 parse_s16(const char *s, const char *what) {
    long v = parse_long(s);
    if (v < -32768 || v > 32767) {
        die("%s %ld out of s16 range", what, v);
    }
    return (s16) v;
}

/* Read the next non-blank, non-comment line; strips trailing whitespace.
 * Returns NULL at EOF. */
static char *next_line(FILE *f, char *buf, size_t bufsz) {
    while (fgets(buf, (int) bufsz, f) != NULL) {
        size_t len;
        sLineNo++;
        if (strchr(buf, '\n') == NULL && !feof(f)) {
            die("line too long (limit %lu bytes)", (unsigned long) bufsz);
        }
        {
            char *hash = strchr(buf, '#');
            if (hash != NULL) {
                *hash = '\0';
            }
        }
        len = strlen(buf);
        while (len > 0 && isspace((unsigned char) buf[len - 1])) {
            buf[--len] = '\0';
        }
        {
            char *s = buf;
            while (*s != '\0' && isspace((unsigned char) *s)) {
                s++;
            }
            if (*s == '\0') {
                continue;
            }
            if (s != buf) {
                memmove(buf, s, strlen(s) + 1);
            }
        }
        return buf;
    }
    return NULL;
}

#define MAX_TOKENS 16

static int tokenize(char *line, char *tok[], int max_tok) {
    int n = 0;
    char *p = line;
    while (*p != '\0') {
        while (*p != '\0' && isspace((unsigned char) *p)) {
            p++;
        }
        if (*p == '\0') {
            break;
        }
        if (n >= max_tok) {
            die("too many tokens on line");
        }
        tok[n++] = p;
        while (*p != '\0' && !isspace((unsigned char) *p)) {
            p++;
        }
        if (*p != '\0') {
            *p++ = '\0';
        }
    }
    return n;
}

static void expect_ntok(int got, int want, const char *directive) {
    if (got != want) {
        die("'%s' expects %d arguments, got %d", directive, want - 1, got - 1);
    }
}

static void parse_scenario(const char *path) {
    FILE *f = fopen(path, "r");
    char buf[4096];
    char *line;
    char *tok[MAX_TOKENS];
    int nt;

    sInputPath = path;
    sLineNo = 0;
    if (f == NULL) {
        die("cannot open input file '%s'", path);
    }

    line = next_line(f, buf, sizeof buf);
    while (line != NULL) {
        nt = tokenize(line, tok, MAX_TOKENS);
        if (nt == 0) {
            line = next_line(f, buf, sizeof buf);
            continue;
        }
        if (strcmp(tok[0], "version") == 0) {
            long v;
            expect_ntok(nt, 2, "version");
            v = parse_long(tok[1]);
            if (v != 1) {
                die("unsupported scenario version %ld (expected 1)", v);
            }
            sScen.have_version = 1;
        } else if (strcmp(tok[0], "terrain") == 0) {
            long t;
            expect_ntok(nt, 2, "terrain");
            t = parse_long(tok[1]);
            if (t < 0 || t > TERRAIN_MASK) {
                die("terrain %ld out of range 0..%d", t, TERRAIN_MASK);
            }
            sScen.terrain = (int) t;
            sScen.have_terrain = 1;
        } else if (strcmp(tok[0], "verts") == 0) {
            long v, i;
            expect_ntok(nt, 2, "verts");
            v = parse_long(tok[1]);
            if (v < 0 || v > MAX_VERTS) {
                die("verts %ld out of range 0..%d", v, MAX_VERTS);
            }
            sScen.nverts = (int) v;
            for (i = 0; i < v; i++) {
                int n2;
                line = next_line(f, buf, sizeof buf);
                if (line == NULL) {
                    die("unexpected EOF inside verts (wanted %ld)", v);
                }
                n2 = tokenize(line, tok, MAX_TOKENS);
                expect_ntok(n2, 3, "vertex");
                sScen.verts[i][0] = parse_s16(tok[0], "vertex x");
                sScen.verts[i][1] = parse_s16(tok[1], "vertex y");
                sScen.verts[i][2] = parse_s16(tok[2], "vertex z");
            }
            sScen.have_verts = 1;
        } else if (strcmp(tok[0], "tris") == 0) {
            long t, i;
            expect_ntok(nt, 2, "tris");
            t = parse_long(tok[1]);
            if (t < 0 || t > MAX_TRIS) {
                die("tris %ld out of range 0..%d", t, MAX_TRIS);
            }
            sScen.ntris = (int) t;
            for (i = 0; i < t; i++) {
                int n2, k;
                long idx;
                line = next_line(f, buf, sizeof buf);
                if (line == NULL) {
                    die("unexpected EOF inside tris (wanted %ld)", t);
                }
                n2 = tokenize(line, tok, MAX_TOKENS);
                expect_ntok(n2, 4, "triangle");
                sScen.tritype[i] = parse_s16(tok[0], "triangle type");
                for (k = 0; k < 3; k++) {
                    idx = parse_long(tok[1 + k]);
                    if (idx < 0 || idx >= sScen.nverts) {
                        die("triangle %ld index %ld out of range (have %d verts)",
                            i, idx, sScen.nverts);
                    }
                    sScen.triidx[i][k] = (s16) idx;
                }
            }
            sScen.have_tris = 1;
        } else if (strcmp(tok[0], "water") == 0) {
            expect_ntok(nt, 2, "water");
            if (strcmp(tok[1], "none") == 0) {
                sScen.water_none = 1;
            } else {
                sScen.water_none = 0;
                sScen.water_level = parse_s16(tok[1], "water level");
            }
            sScen.have_water = 1;
        } else if (strcmp(tok[0], "start") == 0) {
            unsigned long a;
            expect_ntok(nt, 6, "start");
            sScen.start_pos[0] = parse_double(tok[1]);
            sScen.start_pos[1] = parse_double(tok[2]);
            sScen.start_pos[2] = parse_double(tok[3]);
            sScen.start_yaw_deg = parse_double(tok[4]);
            a = parse_base0(tok[5]);
            if (a > 0xFFFFFFFFUL) {
                die("start action out of u32 range");
            }
            sScen.start_action = (u32) a;
            sScen.have_start = 1;
        } else if (strcmp(tok[0], "poke") == 0) {
            expect_ntok(nt, 3, "poke");
            if (strcmp(tok[1], "forward_vel") == 0) {
                sScen.poke_forward_vel = parse_double(tok[2]);
                sScen.have_poke_forward_vel = 1;
            } else {
                die("unknown poke '%s' (only 'forward_vel' is supported)", tok[1]);
            }
        } else if (strcmp(tok[0], "frames") == 0) {
            long n, i;
            expect_ntok(nt, 2, "frames");
            n = parse_long(tok[1]);
            if (n < 0 || n > 1000000) {
                die("frames %ld out of range", n);
            }
            sScen.nframes = (int) n;
            sScen.frames = (struct FrameInput *) calloc((size_t) n, sizeof(struct FrameInput));
            if (n > 0 && sScen.frames == NULL) {
                die("out of memory for %ld frames", n);
            }
            for (i = 0; i < n; i++) {
                int n2;
                long sx, sy, buttons;
                line = next_line(f, buf, sizeof buf);
                if (line == NULL) {
                    die("unexpected EOF inside frames (wanted %ld)", n);
                }
                n2 = tokenize(line, tok, MAX_TOKENS);
                expect_ntok(n2, 4, "frame");
                sx = parse_long(tok[0]);
                sy = parse_long(tok[1]);
                if (sx < -32768 || sx > 32767 || sy < -32768 || sy > 32767) {
                    die("frame %ld stick out of s16 range", i);
                }
                buttons = (long) parse_base0(tok[2]);
                if (buttons < 0 || buttons > 0xFFFF) {
                    die("frame %ld buttons out of u16 range", i);
                }
                sScen.frames[i].sx = (s16) sx;
                sScen.frames[i].sy = (s16) sy;
                sScen.frames[i].buttons = (u16) buttons;
                sScen.frames[i].camyaw_deg = parse_double(tok[3]);
            }
            sScen.have_frames = 1;
            /* frames must be the last directive; anything else is an error */
            line = next_line(f, buf, sizeof buf);
            if (line != NULL) {
                die("unexpected content after frames block");
            }
            break;
        } else {
            die("unknown directive '%s'", tok[0]);
        }
        line = next_line(f, buf, sizeof buf);
    }
    fclose(f);

    if (!sScen.have_version) die("missing 'version' directive");
    if (!sScen.have_terrain) die("missing 'terrain' directive");
    if (!sScen.have_verts) die("missing 'verts' directive");
    if (!sScen.have_tris) die("missing 'tris' directive");
    if (!sScen.have_water) die("missing 'water' directive");
    if (!sScen.have_start) die("missing 'start' directive");
    if (!sScen.have_frames) die("missing 'frames' directive");
}

/* ------------------------------------------------------------------ */
/* Terrain + water setup                                               */
/* ------------------------------------------------------------------ */

static TerrainData sTerrain[TERRAIN_CAP];
static size_t sTerrainLen;

static void emit1(TerrainData v, size_t *p) {
    if (*p >= TERRAIN_CAP) {
        die("terrain buffer overflow");
    }
    sTerrain[(*p)++] = v;
}

/* COL_VERTEX(x,y,z) expands to "x, y, z": use it in a brace initializer,
 * exactly as the decomp's collision.inc.c files do. */
static void emit_vertex(s16 x, s16 y, s16 z, size_t *p) {
    TerrainData e[3] = { COL_VERTEX(x, y, z) };
    emit1(e[0], p);
    emit1(e[1], p);
    emit1(e[2], p);
}

/* COL_TRI_INIT(type,n) expands to "type, n". */
static void emit_tri_init(s16 type, s16 n, size_t *p) {
    TerrainData e[2] = { COL_TRI_INIT(type, n) };
    emit1(e[0], p);
    emit1(e[1], p);
}

/* COL_TRI(a,b,c) expands to "a, b, c". */
static void emit_tri(s16 a, s16 b, s16 c, size_t *p) {
    TerrainData e[3] = { COL_TRI(a, b, c) };
    emit1(e[0], p);
    emit1(e[1], p);
    emit1(e[2], p);
}

static void build_terrain(void) {
    size_t p = 0;
    int i, j, k;

    emit1(COL_INIT(), &p);
    emit1(COL_VERTEX_INIT(sScen.nverts), &p);
    for (i = 0; i < sScen.nverts; i++) {
        emit_vertex(sScen.verts[i][0], sScen.verts[i][1], sScen.verts[i][2], &p);
    }
    emit1(COL_TRI_STOP(), &p);
    /* Group consecutive same-type tris, mirroring the decomp layout. */
    i = 0;
    while (i < sScen.ntris) {
        j = i;
        while (j < sScen.ntris && sScen.tritype[j] == sScen.tritype[i]) {
            j++;
        }
        if (j - i > 32767) {
            die("too many consecutive same-type tris");
        }
        emit_tri_init(sScen.tritype[i], (s16)(j - i), &p);
        for (k = i; k < j; k++) {
            emit_tri(sScen.triidx[k][0], sScen.triidx[k][1], sScen.triidx[k][2], &p);
        }
        i = j;
    }
    emit1(COL_END(), &p);
    sTerrainLen = p;
}

/* Phase-1 simplification: one uniform water region covering the scenario
 * geometry's bounding box (see README). find_water_level indexes
 * gEnvironmentRegions as: [numRegions, val, loX, loZ, hiX, hiZ, level, ...]
 * with val < 50 meaning water. */
static TerrainData sEnvRegions[8];

static void setup_water(void) {
    if (sScen.water_none) {
        sEnvRegions[0] = 0; /* numRegions = 0: no water anywhere */
    } else {
        long loX, loZ, hiX, hiZ;
        int i;
        loX = hiX = sScen.verts[0][0];
        loZ = hiZ = sScen.verts[0][2];
        for (i = 1; i < sScen.nverts; i++) {
            if (sScen.verts[i][0] < loX) loX = sScen.verts[i][0];
            if (sScen.verts[i][0] > hiX) hiX = sScen.verts[i][0];
            if (sScen.verts[i][2] < loZ) loZ = sScen.verts[i][2];
            if (sScen.verts[i][2] > hiZ) hiZ = sScen.verts[i][2];
        }
        /* 1-unit margin: find_water_level uses strict inequalities. */
        loX -= 1; loZ -= 1; hiX += 1; hiZ += 1;
        if (loX < -32768) loX = -32768;
        if (loZ < -32768) loZ = -32768;
        if (hiX > 32767) hiX = 32767;
        if (hiZ > 32767) hiZ = 32767;
        sEnvRegions[0] = 1;                    /* numRegions */
        sEnvRegions[1] = 0;                    /* val < 50 => water */
        sEnvRegions[2] = (TerrainData) loX;
        sEnvRegions[3] = (TerrainData) loZ;
        sEnvRegions[4] = (TerrainData) hiX;
        sEnvRegions[5] = (TerrainData) hiZ;
        sEnvRegions[6] = sScen.water_level;    /* level */
    }
    /* NB: load_area_terrain() resets gEnvironmentRegions to NULL, so this
     * must be assigned after that call. */
    gEnvironmentRegions = sEnvRegions;
}

/* ------------------------------------------------------------------ */
/* CSV output                                                          */
/* ------------------------------------------------------------------ */

static const char *CSV_HEADER =
    "frame,stick_x,stick_y,buttons,cam_yaw,"
    "pos_x,pos_y,pos_z,vel_x,vel_y,vel_z,"
    "forward_vel,slide_vel_x,slide_vel_z,slide_yaw,"
    "face_pitch,face_yaw,face_roll,angle_vel_x,angle_vel_y,angle_vel_z,"
    "intended_yaw,intended_mag,"
    "action,prev_action,action_state,action_timer,action_arg,input,"
    "floor_height,floor_angle,floor_normal_y,floor_type,ceil_height,"
    "wall_normal_yaw,water_level,wall_kick_timer,double_jump_timer,"
    "squish_timer,quicksand_depth,peak_height,health,hurt_counter,"
    "invinc_timer,flags,anim_id,anim_frame,anim_accel,anim_frame_accel_assist";

static void write_csv_row(FILE *out, int frame, const struct FrameInput *in) {
    struct MarioState *m = gMarioState;
    struct Controller *c = &gControllers[0];
    struct AnimInfo *ai = &gMarioObject->header.gfx.animInfo;
    int floor_type = (m->floor != NULL) ? (int) m->floor->type : -1;
    double floor_ny = (m->floor != NULL) ? (double) m->floor->normal.y : 0.0;

    fprintf(out,
            "%d,%.9g,%.9g,%d,%g,"
            "%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,"
            "%.9g,%.9g,%.9g,%d,"
            "%d,%d,%d,%d,%d,%d,"
            "%d,%.9g,"
            "0x%08X,0x%08X,%d,%d,0x%08X,0x%08X,"
            "%.9g,%d,%.9g,%d,%.9g,",
            frame,
            (double) c->stickX, (double) c->stickY,
            (int) c->buttonDown, in->camyaw_deg,
            (double) m->pos[0], (double) m->pos[1], (double) m->pos[2],
            (double) m->vel[0], (double) m->vel[1], (double) m->vel[2],
            (double) m->forwardVel,
            (double) m->slideVelX, (double) m->slideVelZ,
            (int) m->slideYaw,
            (int) m->faceAngle[0], (int) m->faceAngle[1], (int) m->faceAngle[2],
            (int) m->angleVel[0], (int) m->angleVel[1], (int) m->angleVel[2],
            (int) m->intendedYaw, (double) m->intendedMag,
            (unsigned int) m->action, (unsigned int) m->prevAction,
            (int) m->actionState, (int) m->actionTimer,
            (unsigned int) m->actionArg, (unsigned int) m->input,
            (double) m->floorHeight, (int) m->floorAngle,
            floor_ny, floor_type, (double) m->ceilHeight);

    if (m->wall != NULL) {
        /* atan2s(y, x): decomp measures the angle from +y toward +x. */
        fprintf(out, "%d,", (int) atan2s(m->wall->normal.z, m->wall->normal.x));
    } else {
        fputs("none,", out);
    }

    fprintf(out,
            "%d,%d,%d,%d,%.9g,%.9g,%d,%d,%d,0x%08X,%d,%d,%d,%d\n",
            (int) m->waterLevel,
            (int) m->wallKickTimer, (int) m->doubleJumpTimer, (int) m->squishTimer,
            (double) m->quicksandDepth, (double) m->peakHeight,
            (int) m->health, (int) m->hurtCounter, (int) m->invincTimer,
            (unsigned int) m->flags,
            (int) gOracleLastAnimId,
            (int) ai->animFrame, (int) ai->animAccel,
            (int) ai->animFrameAccelAssist);
}

/* ------------------------------------------------------------------ */
/* main                                                                */
/* ------------------------------------------------------------------ */

static struct Area sArea;
static struct Camera sCamera;
static struct Object sMarioObject;
static struct MarioState sMarioState;
static struct Animation sAnimTarget; /* animList->bufTarget (see stubs.c) */

static s16 s16_from_deg(double deg) {
    long v = lround(deg * 65536.0 / 360.0) & 0xFFFFL;
    return (s16)(v >= 0x8000L ? v - 0x10000L : v);
}

static void usage(void) {
    fprintf(stderr, "usage: oracle --in <file.oin> --out <file.csv>\n");
    exit(1);
}

int main(int argc, char **argv) {
    const char *in_path = NULL;
    const char *out_path = NULL;
    FILE *out;
    int i, n;
    u16 prev_buttons;

    for (i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--in") == 0 && i + 1 < argc) {
            in_path = argv[++i];
        } else if (strcmp(argv[i], "--out") == 0 && i + 1 < argc) {
            out_path = argv[++i];
        } else {
            usage();
        }
    }
    if (in_path == NULL || out_path == NULL) {
        usage();
    }

    parse_scenario(in_path);
    build_terrain();

    /* Allocate the surface/node pools first (the real game does this at
     * level init); load_area_terrain assumes they exist. This NULLs
     * gEnvironmentRegions, so water regions are installed afterwards. */
    alloc_surface_pools();
    load_area_terrain(0, sTerrain, NULL, NULL);
    setup_water();

    /* Area / camera. */
    memset(&sArea, 0, sizeof sArea);
    memset(&sCamera, 0, sizeof sCamera);
    gCurrentArea = &sArea;
    sArea.camera = &sCamera;
    sArea.terrainType = (u16) sScen.terrain;

    /* Mario object + state. */
    memset(&sMarioObject, 0, sizeof sMarioObject);
    memset(&sMarioState, 0, sizeof sMarioState);
    gMarioObject = &sMarioObject;
    gMarioState = &sMarioState;

    /* Spawn info feeds init_mario()'s water/floor/pos setup. */
    gMarioSpawnInfo = &gPlayerSpawnInfos[0];
    gMarioSpawnInfo->startPos[0] = (s16) llround(sScen.start_pos[0]);
    gMarioSpawnInfo->startPos[1] = (s16) llround(sScen.start_pos[1]);
    gMarioSpawnInfo->startPos[2] = (s16) llround(sScen.start_pos[2]);
    gMarioSpawnInfo->startAngle[1] = s16_from_deg(sScen.start_yaw_deg);

    /* Fields the real init_mario() reads but never sets itself (the real
     * game sets them in init_mario_from_save_file()). */
    gMarioState->marioBodyState = &gBodyStates[0];
    gMarioState->statusForCamera = &gPlayerCameraState[0];
    gMarioState->spawnInfo = &gPlayerSpawnInfos[0];
    gMarioState->controller = &gControllers[0];

    init_mario();

    /* Explicit post-init overrides (init_mario() does not set these). */
    gMarioState->controller = &gControllers[0];
    gMarioState->health = 0x880;
    gMarioState->animList = &gMarioAnimsBuf;
    gMarioAnimsBuf.bufTarget = &sAnimTarget;
    gMarioState->unkB0 = 0xBD;
    gMarioState->spawnInfo = &gPlayerSpawnInfos[0];
    gMarioState->statusForCamera = &gPlayerCameraState[0];
    gMarioState->marioBodyState = &gBodyStates[0];

    gMarioState->pos[0] = (f32) sScen.start_pos[0];
    gMarioState->pos[1] = (f32) sScen.start_pos[1];
    gMarioState->pos[2] = (f32) sScen.start_pos[2];
    gMarioState->faceAngle[0] = 0;
    gMarioState->faceAngle[1] = s16_from_deg(sScen.start_yaw_deg);
    gMarioState->faceAngle[2] = 0;
    gMarioState->action = sScen.start_action;
    gMarioState->prevAction = sScen.start_action;

    gMarioObject->header.gfx.pos[0] = gMarioState->pos[0];
    gMarioObject->header.gfx.pos[1] = gMarioState->pos[1];
    gMarioObject->header.gfx.pos[2] = gMarioState->pos[2];
    gMarioObject->oPosX = gMarioState->pos[0];
    gMarioObject->oPosY = gMarioState->pos[1];
    gMarioObject->oPosZ = gMarioState->pos[2];

    if (sScen.have_poke_forward_vel) {
        gMarioState->forwardVel = (f32) sScen.poke_forward_vel;
    }

    out = fopen(out_path, "w");
    if (out == NULL) {
        fprintf(stderr, "oracle: error: cannot open output file '%s'\n", out_path);
        return 1;
    }
    fprintf(out, "%s\n", CSV_HEADER);

    n = sScen.nframes;
    prev_buttons = 0;
    for (i = 0; i < n; i++) {
        struct Controller *c = &gControllers[0];
        struct AnimInfo *ai = &gMarioObject->header.gfx.animInfo;

        c->rawStickX = sScen.frames[i].sx;
        c->rawStickY = sScen.frames[i].sy;
        c->buttonDown = sScen.frames[i].buttons;
        c->buttonPressed = (u16)(sScen.frames[i].buttons & (u16) ~prev_buttons);
        prev_buttons = sScen.frames[i].buttons;
        adjust_analog_stick(c);

        gMarioState->area->camera->yaw = s16_from_deg(sScen.frames[i].camyaw_deg);
        gGlobalTimer++;

        execute_mario_action(gMarioObject);

        /* Advance animation once, emulating geo_set_animation_globals()
         * (src/game/rendering_graph_node.c:608). The curAnim NULL guard is a
         * harness-only safety net: with working stubs curAnim is always set
         * by set_mario_animation before the first frame that needs it. */
        if (ai->curAnim != NULL) {
            ai->animFrame = geo_update_animation_frame(ai, &ai->animFrameAccelAssist);
        }
        ai->animTimer = gAreaUpdateCounter;
        gAreaUpdateCounter++;

        write_csv_row(out, i, &sScen.frames[i]);
    }

    if (fclose(out) != 0) {
        fprintf(stderr, "oracle: error: failed writing '%s'\n", out_path);
        return 1;
    }
    return 0;
}
