//! Animation headers and ids ported from the decomp.
//!
//! DO NOT EDIT BY HAND. Regenerate with `tools/gen_anim_table.py`
//! (decomp commit pinned in `oracle/DECOMP_COMMIT`).
//!
//! Headers: `assets/anims/anim_*.inc.c` (the 5-s16 header of each
//! `struct Animation`: flags, animYTransDivisor, startFrame, loopStart,
//! loopEnd). Ids: `include/mario_animation_ids.h` (`enum MarioAnimID`;
//! the hex suffix of the anim array name is the id).
//! See `crate::anim` for the ported animation state machine.

/// One animation header.
/// decomp: `struct Animation` head (`include/types.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimHeader {
    /// `ANIM_FLAG_*` bits (`include/types.h:80-82`).
    pub flags: i16,
    /// `animYTransDivisor` (renderer-only; carried for completeness).
    pub y_trans_divisor: i16,
    pub start_frame: i16,
    pub loop_start: i16,
    pub loop_end: i16,
}

/// `ANIM_FLAG_*` bits. decomp: `include/types.h:80-82`.
pub const ANIM_FLAG_NOLOOP: i16 = 1;
pub const ANIM_FLAG_BACKWARD: i16 = 2;
pub const ANIM_FLAG_2: i16 = 4;

/// Number of animation headers in [`ANIM_TABLE`] (ids 0..208).
pub const ANIM_COUNT: usize = 209;

/// Animation headers indexed by animation id.
/// decomp: `assets/anims/anim_*.inc.c`.
pub const ANIM_TABLE: [AnimHeader; ANIM_COUNT] = [
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 34 }, // 0x00
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 80 }, // 0x01
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 1 }, // 0x02
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 64 }, // 0x03
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 33 }, // 0x04
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 90 }, // 0x05
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 12 }, // 0x06
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 1 }, // 0x07
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 1, loop_start: 0, loop_end: 20 }, // 0x08
    AnimHeader { flags: 16, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 52 }, // 0x09
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 36 }, // 0x0a
    AnimHeader { flags: 17, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 17 }, // 0x0b
    AnimHeader { flags: 19, y_trans_divisor: 189, start_frame: 16, loop_start: 0, loop_end: 17 }, // 0x0c
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 112 }, // 0x0d
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 1 }, // 0x0e
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 1 }, // 0x0f
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 1, loop_start: 0, loop_end: 18 }, // 0x10
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 16 }, // 0x11
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 18 }, // 0x12
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 23 }, // 0x13
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 15 }, // 0x14
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 15 }, // 0x15
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 100 }, // 0x16
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 76 }, // 0x17
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 42, loop_start: 0, loop_end: 100 }, // 0x18
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 86 }, // 0x19
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 17 }, // 0x1a
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 64 }, // 0x1b
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0x1c
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 35 }, // 0x1d
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 30 }, // 0x1e
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 21 }, // 0x1f
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 96 }, // 0x20
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 35 }, // 0x21
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 27 }, // 0x22
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 100 }, // 0x23
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 120 }, // 0x24
    AnimHeader { flags: 65, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 100 }, // 0x25
    AnimHeader { flags: 65, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 94 }, // 0x26
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 112 }, // 0x27
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 15 }, // 0x28
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0x29
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 40 }, // 0x2a
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 40 }, // 0x2b
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 80 }, // 0x2c
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 1 }, // 0x2d
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 47 }, // 0x2e
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 100 }, // 0x2f
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 90 }, // 0x30
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 140 }, // 0x31
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 96 }, // 0x32
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 50 }, // 0x33
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 16 }, // 0x34
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 54 }, // 0x35
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 10, loop_start: 0, loop_end: 46 }, // 0x36
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 100 }, // 0x37
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 12 }, // 0x38
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 155 }, // 0x39
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 6 }, // 0x3a
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 16 }, // 0x3b
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 11 }, // 0x3c
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 10, loop_start: 0, loop_end: 11 }, // 0x3d
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 155 }, // 0x3e
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 61 }, // 0x3f
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 19 }, // 0x40
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0x41
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 2, loop_start: 0, loop_end: 16 }, // 0x42
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 2 }, // 0x43
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 15 }, // 0x44
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 1 }, // 0x45
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 1, loop_start: 0, loop_end: 14 }, // 0x46
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 11 }, // 0x47
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 77 }, // 0x48
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 16 }, // 0x49
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 8 }, // 0x4a
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 18 }, // 0x4b
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 9 }, // 0x4c
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 12 }, // 0x4d
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 22, loop_start: 0, loop_end: 38 }, // 0x4e
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 21 }, // 0x4f
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 6 }, // 0x50
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 14 }, // 0x51
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 11 }, // 0x52
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 25 }, // 0x53
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 30 }, // 0x54
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 152 }, // 0x55
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 1 }, // 0x56
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 9, loop_start: 0, loop_end: 24 }, // 0x57
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 20 }, // 0x58
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 37 }, // 0x59
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 38 }, // 0x5a
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0x5b
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 20 }, // 0x5c
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 19 }, // 0x5d
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 155 }, // 0x5e
    AnimHeader { flags: 9, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 60 }, // 0x5f
    AnimHeader { flags: 9, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 50 }, // 0x60
    AnimHeader { flags: 9, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 147 }, // 0x61
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0x62
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 33 }, // 0x63
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 18 }, // 0x64
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 21 }, // 0x65
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 26 }, // 0x66
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 5 }, // 0x67
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 4 }, // 0x68
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0x69
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 13 }, // 0x6a
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 11 }, // 0x6b
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 27 }, // 0x6c
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 18 }, // 0x6d
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 16 }, // 0x6e
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0x6f
    AnimHeader { flags: 2, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0x70
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 24 }, // 0x71
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 72 }, // 0x72
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 72 }, // 0x73
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 24 }, // 0x74
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 24 }, // 0x75
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 100 }, // 0x76
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 100 }, // 0x77
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 149 }, // 0x78
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 53 }, // 0x79
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 6 }, // 0x7a
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 30 }, // 0x7b
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 30 }, // 0x7c
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 40 }, // 0x7d
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 90 }, // 0x7e
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 64 }, // 0x7f
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 64 }, // 0x80
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 50 }, // 0x81
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 67 }, // 0x82
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 59 }, // 0x83
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 64 }, // 0x84
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 40 }, // 0x85
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 40 }, // 0x86
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 50 }, // 0x87
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 20 }, // 0x88
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 20, loop_start: 0, loop_end: 20 }, // 0x89
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 40 }, // 0x8a
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 37 }, // 0x8b
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 8 }, // 0x8c
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 14 }, // 0x8d
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 1 }, // 0x8e
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 1, loop_start: 0, loop_end: 20 }, // 0x8f
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 15 }, // 0x90
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 8 }, // 0x91
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 27, loop_start: 0, loop_end: 116 }, // 0x92
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0x93
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 1 }, // 0x94
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 5 }, // 0x95
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0x96
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 6 }, // 0x97
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 90 }, // 0x98
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 92 }, // 0x99
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 8 }, // 0x9a
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 8 }, // 0x9b
    AnimHeader { flags: 9, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 64 }, // 0x9c
    AnimHeader { flags: 9, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 26 }, // 0x9d
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 50 }, // 0x9e
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 16 }, // 0x9f
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0xa0
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 24 }, // 0xa1
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 23 }, // 0xa2
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 30 }, // 0xa3
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 30 }, // 0xa4
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 80 }, // 0xa5
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 70 }, // 0xa6
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 40 }, // 0xa7
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 50 }, // 0xa8
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 20 }, // 0xa9
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 13 }, // 0xaa
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0xab
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 8, loop_start: 0, loop_end: 24 }, // 0xac
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 25 }, // 0xad
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 11 }, // 0xae
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 25 }, // 0xaf
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 5 }, // 0xb0
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 28 }, // 0xb1
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 30 }, // 0xb2
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 83 }, // 0xb3
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 24 }, // 0xb4
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 20 }, // 0xb5
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 19, loop_start: 0, loop_end: 20 }, // 0xb6
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 40 }, // 0xb7
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 50 }, // 0xb8
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 25 }, // 0xb9
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 20 }, // 0xba
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 97 }, // 0xbb
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 1 }, // 0xbc
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 1, loop_start: 0, loop_end: 18 }, // 0xbd
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 10 }, // 0xbe
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 24 }, // 0xbf
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 32 }, // 0xc0
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 36 }, // 0xc1
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 90 }, // 0xc2
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 30 }, // 0xc3
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 30 }, // 0xc4
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 30 }, // 0xc5
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 50 }, // 0xc6
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 50 }, // 0xc7
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 6, loop_start: 0, loop_end: 20 }, // 0xc8
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 21 }, // 0xc9
    AnimHeader { flags: 0, y_trans_divisor: 189, start_frame: 25, loop_start: 0, loop_end: 30 }, // 0xca
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 17 }, // 0xcb
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 1 }, // 0xcc
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 63 }, // 0xcd
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 18 }, // 0xce
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 42 }, // 0xcf
    AnimHeader { flags: 1, y_trans_divisor: 189, start_frame: 0, loop_start: 0, loop_end: 14 }, // 0xd0
];

/// Look up an animation header by id.
///
/// Mirrors the oracle stub's `load_patchable_table`: out-of-range ids
/// (including negative ones) fall back to id 0.
pub fn anim_header(id: i32) -> AnimHeader {
    if id < 0 || (id as usize) >= ANIM_COUNT {
        ANIM_TABLE[0]
    } else {
        ANIM_TABLE[id as usize]
    }
}

/// `MARIO_ANIM_*` ids. decomp: `include/mario_animation_ids.h`.
pub const MARIO_ANIM_SLOW_LEDGE_GRAB: i32 = 0; // 0x00
pub const MARIO_ANIM_FALL_OVER_BACKWARDS: i32 = 1; // 0x01
pub const MARIO_ANIM_BACKWARD_AIR_KB: i32 = 2; // 0x02
pub const MARIO_ANIM_DYING_ON_BACK: i32 = 3; // 0x03
pub const MARIO_ANIM_BACKFLIP: i32 = 4; // 0x04
pub const MARIO_ANIM_CLIMB_UP_POLE: i32 = 5; // 0x05
pub const MARIO_ANIM_GRAB_POLE_SHORT: i32 = 6; // 0x06
pub const MARIO_ANIM_GRAB_POLE_SWING_PART1: i32 = 7; // 0x07
pub const MARIO_ANIM_GRAB_POLE_SWING_PART2: i32 = 8; // 0x08
pub const MARIO_ANIM_HANDSTAND_IDLE: i32 = 9; // 0x09
pub const MARIO_ANIM_HANDSTAND_JUMP: i32 = 10; // 0x0a
pub const MARIO_ANIM_START_HANDSTAND: i32 = 11; // 0x0b
pub const MARIO_ANIM_RETURN_FROM_HANDSTAND: i32 = 12; // 0x0c
pub const MARIO_ANIM_IDLE_ON_POLE: i32 = 13; // 0x0d
pub const MARIO_ANIM_A_POSE: i32 = 14; // 0x0e
pub const MARIO_ANIM_SKID_ON_GROUND: i32 = 15; // 0x0f
pub const MARIO_ANIM_STOP_SKID: i32 = 16; // 0x10
pub const MARIO_ANIM_CROUCH_FROM_FAST_LONGJUMP: i32 = 17; // 0x11
pub const MARIO_ANIM_CROUCH_FROM_SLOW_LONGJUMP: i32 = 18; // 0x12
pub const MARIO_ANIM_FAST_LONGJUMP: i32 = 19; // 0x13
pub const MARIO_ANIM_SLOW_LONGJUMP: i32 = 20; // 0x14
pub const MARIO_ANIM_AIRBORNE_ON_STOMACH: i32 = 21; // 0x15
pub const MARIO_ANIM_WALK_WITH_LIGHT_OBJ: i32 = 22; // 0x16
pub const MARIO_ANIM_RUN_WITH_LIGHT_OBJ: i32 = 23; // 0x17
pub const MARIO_ANIM_SLOW_WALK_WITH_LIGHT_OBJ: i32 = 24; // 0x18
pub const MARIO_ANIM_SHIVERING_WARMING_HAND: i32 = 25; // 0x19
pub const MARIO_ANIM_SHIVERING_RETURN_TO_IDLE: i32 = 26; // 0x1a
pub const MARIO_ANIM_SHIVERING: i32 = 27; // 0x1b
pub const MARIO_ANIM_CLIMB_DOWN_LEDGE: i32 = 28; // 0x1c
pub const MARIO_ANIM_CREDITS_WAVING: i32 = 29; // 0x1d
pub const MARIO_ANIM_CREDITS_LOOK_UP: i32 = 30; // 0x1e
pub const MARIO_ANIM_CREDITS_RETURN_FROM_LOOK_UP: i32 = 31; // 0x1f
pub const MARIO_ANIM_CREDITS_RAISE_HAND: i32 = 32; // 0x20
pub const MARIO_ANIM_CREDITS_LOWER_HAND: i32 = 33; // 0x21
pub const MARIO_ANIM_CREDITS_TAKE_OFF_CAP: i32 = 34; // 0x22
pub const MARIO_ANIM_CREDITS_START_WALK_LOOK_UP: i32 = 35; // 0x23
pub const MARIO_ANIM_CREDITS_LOOK_BACK_THEN_RUN: i32 = 36; // 0x24
pub const MARIO_ANIM_FINAL_BOWSER_RAISE_HAND_SPIN: i32 = 37; // 0x25
pub const MARIO_ANIM_FINAL_BOWSER_WING_CAP_TAKE_OFF: i32 = 38; // 0x26
pub const MARIO_ANIM_CREDITS_PEACE_SIGN: i32 = 39; // 0x27
pub const MARIO_ANIM_STAND_UP_FROM_LAVA_BOOST: i32 = 40; // 0x28
pub const MARIO_ANIM_FIRE_LAVA_BURN: i32 = 41; // 0x29
pub const MARIO_ANIM_WING_CAP_FLY: i32 = 42; // 0x2a
pub const MARIO_ANIM_HANG_ON_OWL: i32 = 43; // 0x2b
pub const MARIO_ANIM_LAND_ON_STOMACH: i32 = 44; // 0x2c
pub const MARIO_ANIM_AIR_FORWARD_KB: i32 = 45; // 0x2d
pub const MARIO_ANIM_DYING_ON_STOMACH: i32 = 46; // 0x2e
pub const MARIO_ANIM_SUFFOCATING: i32 = 47; // 0x2f
pub const MARIO_ANIM_COUGHING: i32 = 48; // 0x30
pub const MARIO_ANIM_THROW_CATCH_KEY: i32 = 49; // 0x31
pub const MARIO_ANIM_DYING_FALL_OVER: i32 = 50; // 0x32
pub const MARIO_ANIM_IDLE_ON_LEDGE: i32 = 51; // 0x33
pub const MARIO_ANIM_FAST_LEDGE_GRAB: i32 = 52; // 0x34
pub const MARIO_ANIM_HANG_ON_CEILING: i32 = 53; // 0x35
pub const MARIO_ANIM_PUT_CAP_ON: i32 = 54; // 0x36
pub const MARIO_ANIM_TAKE_CAP_OFF_THEN_ON: i32 = 55; // 0x37
pub const MARIO_ANIM_QUICKLY_PUT_CAP_ON: i32 = 56; // 0x38
pub const MARIO_ANIM_HEAD_STUCK_IN_GROUND: i32 = 57; // 0x39
pub const MARIO_ANIM_GROUND_POUND_LANDING: i32 = 58; // 0x3a
pub const MARIO_ANIM_TRIPLE_JUMP_GROUND_POUND: i32 = 59; // 0x3b
pub const MARIO_ANIM_START_GROUND_POUND: i32 = 60; // 0x3c
pub const MARIO_ANIM_GROUND_POUND: i32 = 61; // 0x3d
pub const MARIO_ANIM_BOTTOM_STUCK_IN_GROUND: i32 = 62; // 0x3e
pub const MARIO_ANIM_IDLE_WITH_LIGHT_OBJ: i32 = 63; // 0x3f
pub const MARIO_ANIM_JUMP_LAND_WITH_LIGHT_OBJ: i32 = 64; // 0x40
pub const MARIO_ANIM_JUMP_WITH_LIGHT_OBJ: i32 = 65; // 0x41
pub const MARIO_ANIM_FALL_LAND_WITH_LIGHT_OBJ: i32 = 66; // 0x42
pub const MARIO_ANIM_FALL_WITH_LIGHT_OBJ: i32 = 67; // 0x43
pub const MARIO_ANIM_FALL_FROM_SLIDING_WITH_LIGHT_OBJ: i32 = 68; // 0x44
pub const MARIO_ANIM_SLIDING_ON_BOTTOM_WITH_LIGHT_OBJ: i32 = 69; // 0x45
pub const MARIO_ANIM_STAND_UP_FROM_SLIDING_WITH_LIGHT_OBJ: i32 = 70; // 0x46
pub const MARIO_ANIM_RIDING_SHELL: i32 = 71; // 0x47
pub const MARIO_ANIM_WALKING: i32 = 72; // 0x48
pub const MARIO_ANIM_FORWARD_FLIP: i32 = 73; // 0x49
pub const MARIO_ANIM_JUMP_RIDING_SHELL: i32 = 74; // 0x4a
pub const MARIO_ANIM_LAND_FROM_DOUBLE_JUMP: i32 = 75; // 0x4b
pub const MARIO_ANIM_DOUBLE_JUMP_FALL: i32 = 76; // 0x4c
pub const MARIO_ANIM_SINGLE_JUMP: i32 = 77; // 0x4d
pub const MARIO_ANIM_LAND_FROM_SINGLE_JUMP: i32 = 78; // 0x4e
pub const MARIO_ANIM_AIR_KICK: i32 = 79; // 0x4f
pub const MARIO_ANIM_DOUBLE_JUMP_RISE: i32 = 80; // 0x50
pub const MARIO_ANIM_START_FORWARD_SPINNING: i32 = 81; // 0x51
pub const MARIO_ANIM_THROW_LIGHT_OBJECT: i32 = 82; // 0x52
pub const MARIO_ANIM_FALL_FROM_SLIDE_KICK: i32 = 83; // 0x53
pub const MARIO_ANIM_BEND_KNESS_RIDING_SHELL: i32 = 84; // 0x54
pub const MARIO_ANIM_LEGS_STUCK_IN_GROUND: i32 = 85; // 0x55
pub const MARIO_ANIM_GENERAL_FALL: i32 = 86; // 0x56
pub const MARIO_ANIM_GENERAL_LAND: i32 = 87; // 0x57
pub const MARIO_ANIM_BEING_GRABBED: i32 = 88; // 0x58
pub const MARIO_ANIM_GRAB_HEAVY_OBJECT: i32 = 89; // 0x59
pub const MARIO_ANIM_SLOW_LAND_FROM_DIVE: i32 = 90; // 0x5a
pub const MARIO_ANIM_FLY_FROM_CANNON: i32 = 91; // 0x5b
pub const MARIO_ANIM_MOVE_ON_WIRE_NET_RIGHT: i32 = 92; // 0x5c
pub const MARIO_ANIM_MOVE_ON_WIRE_NET_LEFT: i32 = 93; // 0x5d
pub const MARIO_ANIM_MISSING_CAP: i32 = 94; // 0x5e
pub const MARIO_ANIM_PULL_DOOR_WALK_IN: i32 = 95; // 0x5f
pub const MARIO_ANIM_PUSH_DOOR_WALK_IN: i32 = 96; // 0x60
pub const MARIO_ANIM_UNLOCK_DOOR: i32 = 97; // 0x61
pub const MARIO_ANIM_START_REACH_POCKET: i32 = 98; // 0x62
pub const MARIO_ANIM_REACH_POCKET: i32 = 99; // 0x63
pub const MARIO_ANIM_STOP_REACH_POCKET: i32 = 100; // 0x64
pub const MARIO_ANIM_GROUND_THROW: i32 = 101; // 0x65
pub const MARIO_ANIM_GROUND_KICK: i32 = 102; // 0x66
pub const MARIO_ANIM_FIRST_PUNCH: i32 = 103; // 0x67
pub const MARIO_ANIM_SECOND_PUNCH: i32 = 104; // 0x68
pub const MARIO_ANIM_FIRST_PUNCH_FAST: i32 = 105; // 0x69
pub const MARIO_ANIM_SECOND_PUNCH_FAST: i32 = 106; // 0x6a
pub const MARIO_ANIM_PICK_UP_LIGHT_OBJ: i32 = 107; // 0x6b
pub const MARIO_ANIM_PUSHING: i32 = 108; // 0x6c
pub const MARIO_ANIM_START_RIDING_SHELL: i32 = 109; // 0x6d
pub const MARIO_ANIM_PLACE_LIGHT_OBJ: i32 = 110; // 0x6e
pub const MARIO_ANIM_FORWARD_SPINNING: i32 = 111; // 0x6f
pub const MARIO_ANIM_BACKWARD_SPINNING: i32 = 112; // 0x70
pub const MARIO_ANIM_BREAKDANCE: i32 = 113; // 0x71
pub const MARIO_ANIM_RUNNING: i32 = 114; // 0x72
pub const MARIO_ANIM_RUNNING_UNUSED: i32 = 115; // 0x73
pub const MARIO_ANIM_SOFT_BACK_KB: i32 = 116; // 0x74
pub const MARIO_ANIM_SOFT_FRONT_KB: i32 = 117; // 0x75
pub const MARIO_ANIM_DYING_IN_QUICKSAND: i32 = 118; // 0x76
pub const MARIO_ANIM_IDLE_IN_QUICKSAND: i32 = 119; // 0x77
pub const MARIO_ANIM_MOVE_IN_QUICKSAND: i32 = 120; // 0x78
pub const MARIO_ANIM_ELECTROCUTION: i32 = 121; // 0x79
pub const MARIO_ANIM_SHOCKED: i32 = 122; // 0x7a
pub const MARIO_ANIM_BACKWARD_KB: i32 = 123; // 0x7b
pub const MARIO_ANIM_FORWARD_KB: i32 = 124; // 0x7c
pub const MARIO_ANIM_IDLE_HEAVY_OBJ: i32 = 125; // 0x7d
pub const MARIO_ANIM_STAND_AGAINST_WALL: i32 = 126; // 0x7e
pub const MARIO_ANIM_SIDESTEP_LEFT: i32 = 127; // 0x7f
pub const MARIO_ANIM_SIDESTEP_RIGHT: i32 = 128; // 0x80
pub const MARIO_ANIM_START_SLEEP_IDLE: i32 = 129; // 0x81
pub const MARIO_ANIM_START_SLEEP_SCRATCH: i32 = 130; // 0x82
pub const MARIO_ANIM_START_SLEEP_YAWN: i32 = 131; // 0x83
pub const MARIO_ANIM_START_SLEEP_SITTING: i32 = 132; // 0x84
pub const MARIO_ANIM_SLEEP_IDLE: i32 = 133; // 0x85
pub const MARIO_ANIM_SLEEP_START_LYING: i32 = 134; // 0x86
pub const MARIO_ANIM_SLEEP_LYING: i32 = 135; // 0x87
pub const MARIO_ANIM_DIVE: i32 = 136; // 0x88
pub const MARIO_ANIM_SLIDE_DIVE: i32 = 137; // 0x89
pub const MARIO_ANIM_GROUND_BONK: i32 = 138; // 0x8a
pub const MARIO_ANIM_STOP_SLIDE_LIGHT_OBJ: i32 = 139; // 0x8b
pub const MARIO_ANIM_SLIDE_KICK: i32 = 140; // 0x8c
pub const MARIO_ANIM_CROUCH_FROM_SLIDE_KICK: i32 = 141; // 0x8d
pub const MARIO_ANIM_SLIDE_MOTIONLESS: i32 = 142; // 0x8e
pub const MARIO_ANIM_STOP_SLIDE: i32 = 143; // 0x8f
pub const MARIO_ANIM_FALL_FROM_SLIDE: i32 = 144; // 0x90
pub const MARIO_ANIM_SLIDE: i32 = 145; // 0x91
pub const MARIO_ANIM_TIPTOE: i32 = 146; // 0x92
pub const MARIO_ANIM_TWIRL_LAND: i32 = 147; // 0x93
pub const MARIO_ANIM_TWIRL: i32 = 148; // 0x94
pub const MARIO_ANIM_START_TWIRL: i32 = 149; // 0x95
pub const MARIO_ANIM_STOP_CROUCHING: i32 = 150; // 0x96
pub const MARIO_ANIM_START_CROUCHING: i32 = 151; // 0x97
pub const MARIO_ANIM_CROUCHING: i32 = 152; // 0x98
pub const MARIO_ANIM_CRAWLING: i32 = 153; // 0x99
pub const MARIO_ANIM_STOP_CRAWLING: i32 = 154; // 0x9a
pub const MARIO_ANIM_START_CRAWLING: i32 = 155; // 0x9b
pub const MARIO_ANIM_SUMMON_STAR: i32 = 156; // 0x9c
pub const MARIO_ANIM_RETURN_STAR_APPROACH_DOOR: i32 = 157; // 0x9d
pub const MARIO_ANIM_BACKWARDS_WATER_KB: i32 = 158; // 0x9e
pub const MARIO_ANIM_SWIM_WITH_OBJ_PART1: i32 = 159; // 0x9f
pub const MARIO_ANIM_SWIM_WITH_OBJ_PART2: i32 = 160; // 0xa0
pub const MARIO_ANIM_FLUTTERKICK_WITH_OBJ: i32 = 161; // 0xa1
pub const MARIO_ANIM_WATER_ACTION_END_WITH_OBJ: i32 = 162; // 0xa2
pub const MARIO_ANIM_STOP_GRAB_OBJ_WATER: i32 = 163; // 0xa3
pub const MARIO_ANIM_WATER_IDLE_WITH_OBJ: i32 = 164; // 0xa4
pub const MARIO_ANIM_DROWNING_PART1: i32 = 165; // 0xa5
pub const MARIO_ANIM_DROWNING_PART2: i32 = 166; // 0xa6
pub const MARIO_ANIM_WATER_DYING: i32 = 167; // 0xa7
pub const MARIO_ANIM_WATER_FORWARD_KB: i32 = 168; // 0xa8
pub const MARIO_ANIM_FALL_FROM_WATER: i32 = 169; // 0xa9
pub const MARIO_ANIM_SWIM_PART1: i32 = 170; // 0xaa
pub const MARIO_ANIM_SWIM_PART2: i32 = 171; // 0xab
pub const MARIO_ANIM_FLUTTERKICK: i32 = 172; // 0xac
pub const MARIO_ANIM_WATER_ACTION_END: i32 = 173; // 0xad
pub const MARIO_ANIM_WATER_PICK_UP_OBJ: i32 = 174; // 0xae
pub const MARIO_ANIM_WATER_GRAB_OBJ_PART2: i32 = 175; // 0xaf
pub const MARIO_ANIM_WATER_GRAB_OBJ_PART1: i32 = 176; // 0xb0
pub const MARIO_ANIM_WATER_THROW_OBJ: i32 = 177; // 0xb1
pub const MARIO_ANIM_WATER_IDLE: i32 = 178; // 0xb2
pub const MARIO_ANIM_WATER_STAR_DANCE: i32 = 179; // 0xb3
pub const MARIO_ANIM_RETURN_FROM_WATER_STAR_DANCE: i32 = 180; // 0xb4
pub const MARIO_ANIM_GRAB_BOWSER: i32 = 181; // 0xb5
pub const MARIO_ANIM_SWINGING_BOWSER: i32 = 182; // 0xb6
pub const MARIO_ANIM_RELEASE_BOWSER: i32 = 183; // 0xb7
pub const MARIO_ANIM_HOLDING_BOWSER: i32 = 184; // 0xb8
pub const MARIO_ANIM_HEAVY_THROW: i32 = 185; // 0xb9
pub const MARIO_ANIM_WALK_PANTING: i32 = 186; // 0xba
pub const MARIO_ANIM_WALK_WITH_HEAVY_OBJ: i32 = 187; // 0xbb
pub const MARIO_ANIM_TURNING_PART1: i32 = 188; // 0xbc
pub const MARIO_ANIM_TURNING_PART2: i32 = 189; // 0xbd
pub const MARIO_ANIM_SLIDEFLIP_LAND: i32 = 190; // 0xbe
pub const MARIO_ANIM_SLIDEFLIP: i32 = 191; // 0xbf
pub const MARIO_ANIM_TRIPLE_JUMP_LAND: i32 = 192; // 0xc0
pub const MARIO_ANIM_TRIPLE_JUMP: i32 = 193; // 0xc1
pub const MARIO_ANIM_FIRST_PERSON: i32 = 194; // 0xc2
pub const MARIO_ANIM_IDLE_HEAD_LEFT: i32 = 195; // 0xc3
pub const MARIO_ANIM_IDLE_HEAD_RIGHT: i32 = 196; // 0xc4
pub const MARIO_ANIM_IDLE_HEAD_CENTER: i32 = 197; // 0xc5
pub const MARIO_ANIM_HANDSTAND_LEFT: i32 = 198; // 0xc6
pub const MARIO_ANIM_HANDSTAND_RIGHT: i32 = 199; // 0xc7
pub const MARIO_ANIM_WAKE_FROM_SLEEP: i32 = 200; // 0xc8
pub const MARIO_ANIM_WAKE_FROM_LYING: i32 = 201; // 0xc9
pub const MARIO_ANIM_START_TIPTOE: i32 = 202; // 0xca
pub const MARIO_ANIM_SLIDEJUMP: i32 = 203; // 0xcb
pub const MARIO_ANIM_START_WALLKICK: i32 = 204; // 0xcc
pub const MARIO_ANIM_STAR_DANCE: i32 = 205; // 0xcd
pub const MARIO_ANIM_RETURN_FROM_STAR_DANCE: i32 = 206; // 0xce
pub const MARIO_ANIM_FORWARD_SPINNING_FLIP: i32 = 207; // 0xcf
pub const MARIO_ANIM_TRIPLE_JUMP_FLY: i32 = 208; // 0xd0
