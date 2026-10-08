//! Compact binary trace format for fast CI runs.
//!
//! `stepkit trace pack` converts a directory of CSV traces; the layout is a
//! magic header, a u32 frame count, then each frame as packed little-endian
//! fields. Numeric fields keep exact `f32`/`i16` bit patterns.

use crate::trace::TraceFrame;

const MAGIC: &[u8; 8] = b"STPKTRC\x03"; // v3: added wall_kick_timer, wall_normal_yaw

/// Pack frames into the binary format.
pub fn pack_frames(frames: &[TraceFrame]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + 4 + frames.len() * 96);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    for f in frames {
        out.extend_from_slice(&f.frame.to_le_bytes());
        out.push(f.stick_x as u8);
        out.push(f.stick_y as u8);
        out.extend_from_slice(&f.buttons.to_le_bytes());
        out.extend_from_slice(&f.cam_yaw.to_le_bytes());
        for v in [
            f.pos_x,
            f.pos_y,
            f.pos_z,
            f.vel_x,
            f.vel_y,
            f.vel_z,
            f.fwd_speed,
            f.floor_y,
            f.ceil_y,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [f.face_yaw, f.face_pitch, f.face_roll] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [
            f.action,
            f.prev_action,
            f.land_from,
            f.action_state,
            f.action_timer,
            f.action_arg,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.push(f.wall_hit);
        out.extend_from_slice(&f.wall_kick_timer.to_le_bytes());
        out.extend_from_slice(&f.wall_normal_yaw.to_le_bytes());
        let slot = f.anim_slot.as_bytes();
        out.push(slot.len().min(255) as u8);
        out.extend_from_slice(&slot[..slot.len().min(255)]);
        out.extend_from_slice(&f.anim_frame.to_le_bytes());
    }
    out
}

fn read_u32(data: &[u8], pos: &mut usize) -> Option<u32> {
    let b = data.get(*pos..*pos + 4)?;
    *pos += 4;
    Some(u32::from_le_bytes(b.try_into().ok()?))
}

fn read_u16(data: &[u8], pos: &mut usize) -> Option<u16> {
    let b = data.get(*pos..*pos + 2)?;
    *pos += 2;
    Some(u16::from_le_bytes(b.try_into().ok()?))
}

fn read_i16(data: &[u8], pos: &mut usize) -> Option<i16> {
    read_u16(data, pos).map(|v| v as i16)
}

fn read_f32(data: &[u8], pos: &mut usize) -> Option<f32> {
    read_u32(data, pos).map(f32::from_bits)
}

/// Unpack frames from the binary format.
pub fn unpack_frames(data: &[u8]) -> Option<Vec<TraceFrame>> {
    if data.get(..8) != Some(MAGIC.as_slice()) {
        return None;
    }
    let mut pos = 8;
    let count = read_u32(data, &mut pos)? as usize;
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let frame = read_u32(data, &mut pos)?;
        let stick_x = *data.get(pos)? as i8;
        pos += 1;
        let stick_y = *data.get(pos)? as i8;
        pos += 1;
        let buttons = read_u16(data, &mut pos)?;
        let cam_yaw = read_i16(data, &mut pos)?;
        let pos_x = read_f32(data, &mut pos)?;
        let pos_y = read_f32(data, &mut pos)?;
        let pos_z = read_f32(data, &mut pos)?;
        let vel_x = read_f32(data, &mut pos)?;
        let vel_y = read_f32(data, &mut pos)?;
        let vel_z = read_f32(data, &mut pos)?;
        let fwd_speed = read_f32(data, &mut pos)?;
        let floor_y = read_f32(data, &mut pos)?;
        let ceil_y = read_f32(data, &mut pos)?;
        let face_yaw = read_i16(data, &mut pos)?;
        let face_pitch = read_i16(data, &mut pos)?;
        let face_roll = read_i16(data, &mut pos)?;
        let action = read_u32(data, &mut pos)?;
        let prev_action = read_u32(data, &mut pos)?;
        let land_from = read_u32(data, &mut pos)?;
        let action_state = read_u32(data, &mut pos)?;
        let action_timer = read_u32(data, &mut pos)?;
        let action_arg = read_u32(data, &mut pos)?;
        let wall_hit = *data.get(pos)?;
        pos += 1;
        let wall_kick_timer = read_u32(data, &mut pos)?;
        let wall_normal_yaw = read_i16(data, &mut pos)?;
        let slot_len = *data.get(pos)? as usize;
        pos += 1;
        let slot = core::str::from_utf8(data.get(pos..pos + slot_len)?)
            .ok()?
            .to_string();
        pos += slot_len;
        let anim_frame = read_u32(data, &mut pos)?;
        frames.push(TraceFrame {
            frame,
            stick_x,
            stick_y,
            buttons,
            cam_yaw,
            pos_x,
            pos_y,
            pos_z,
            vel_x,
            vel_y,
            vel_z,
            fwd_speed,
            face_yaw,
            face_pitch,
            face_roll,
            action,
            prev_action,
            land_from,
            action_state,
            action_timer,
            action_arg,
            floor_y,
            ceil_y,
            wall_hit,
            wall_kick_timer,
            wall_normal_yaw,
            anim_slot: slot,
            anim_frame,
        });
    }
    Some(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(i: u32) -> TraceFrame {
        TraceFrame {
            frame: i,
            stick_x: -12,
            stick_y: 80,
            buttons: 3,
            cam_yaw: -100,
            pos_x: 1.5,
            pos_y: -0.25,
            pos_z: 3.75,
            vel_x: 0.125,
            vel_y: -4.0,
            vel_z: 0.0,
            fwd_speed: 48.5,
            face_yaw: 1234,
            face_pitch: -56,
            face_roll: 0,
            action: 0x04000440,
            prev_action: 0x0C400201,
            land_from: 0x0C400201,
            action_state: 2,
            action_timer: 17,
            action_arg: 0,
            floor_y: 0.0,
            ceil_y: 1000.0,
            wall_hit: 0,
            wall_kick_timer: 0,
            wall_normal_yaw: 0,
            anim_slot: "walk_cycle".into(),
            anim_frame: 9,
        }
    }

    #[test]
    fn pack_round_trip_is_exact() {
        let frames: Vec<TraceFrame> = (0..50).map(frame).collect();
        let packed = pack_frames(&frames);
        assert!(packed.len() < 50 * 200);
        let back = unpack_frames(&packed).unwrap();
        assert_eq!(back, frames);
    }

    #[test]
    fn bad_magic_rejected() {
        assert!(unpack_frames(b"garbage").is_none());
    }
}
