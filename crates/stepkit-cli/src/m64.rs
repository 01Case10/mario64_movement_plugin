//! `.m64` (Mupen64 TAS movie) input importer.
//!
//! Layout per the public TASVideos documentation
//! ("EmulatorResources/Mupen/M64"): a 1024-byte header (input data at
//! 0x400 for version 3, 0x200 for versions 1-2), then 4-byte samples of
//! (buttons u16 LE, stick_x i8, stick_y i8) per controller per frame.
//! Converts controller inputs only; never touches game data.

use stepkit_trace::scenario::{Camera, CompareProfile, InputSeg, Scenario, StartState};

const HEADER_V3: usize = 0x400;
const HEADER_V12: usize = 0x200;

/// One decoded input sample.
#[derive(Clone, Copy, Debug)]
pub struct M64Sample {
    pub buttons_m64: u16,
    pub stick_x: i8,
    pub stick_y: i8,
}

/// Map M64 button bits to the core's button bitfield
/// (A=0x0001, B=0x0002, Z=0x0004, R=0x0008, START=0x0010).
fn map_buttons(m64: u16) -> u16 {
    let mut out = 0u16;
    if m64 & 0x8000 != 0 {
        out |= 0x0001;
    } // A
    if m64 & 0x4000 != 0 {
        out |= 0x0002;
    } // B
    if m64 & 0x2000 != 0 {
        out |= 0x0004;
    } // Z
    if m64 & 0x0010 != 0 {
        out |= 0x0008;
    } // R
    if m64 & 0x1000 != 0 {
        out |= 0x0010;
    } // Start
    out
}

/// Parse an .m64 file into samples (controller 1 only).
pub fn parse_m64(data: &[u8]) -> Result<(u32, Vec<M64Sample>), String> {
    if data.len() < 0x20 || &data[0..4] != b"M64\x1A" {
        return Err("not an .m64 file (bad signature)".into());
    }
    let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let num_frames = u32::from_le_bytes(data[0x0C..0x10].try_into().unwrap());
    let num_samples = u32::from_le_bytes(data[0x18..0x1C].try_into().unwrap()) as usize;
    let num_controllers = data[0x15] as usize;
    let base = match version {
        3 => HEADER_V3,
        1 | 2 => HEADER_V12,
        v => return Err(format!("unsupported .m64 version {v}")),
    };
    if num_controllers == 0 {
        return Err("no controllers in movie".into());
    }
    // Samples are grouped per frame: controller 1's sample comes first.
    let stride = 4 * num_controllers;
    let mut samples = Vec::new();
    for i in 0..num_samples.min(num_frames as usize) {
        let off = base + i * stride;
        let bytes: [u8; 4] = data
            .get(off..off + 4)
            .ok_or("truncated input data")?
            .try_into()
            .unwrap();
        let buttons = u16::from_le_bytes([bytes[0], bytes[1]]);
        samples.push(M64Sample {
            buttons_m64: buttons,
            stick_x: bytes[2] as i8,
            // TAS input plugin inverts Y when saving; restore "up = +Y".
            stick_y: (bytes[3] as i8).wrapping_neg(),
        });
    }
    Ok((num_frames, samples))
}

/// Convert samples into a [`Scenario`] (flat ground, fixed camera).
/// `frames` selects a range like "0..1800"; `None` takes everything.
pub fn samples_to_scenario(
    id: &str,
    samples: &[M64Sample],
    frames: Option<(u32, u32)>,
) -> Scenario {
    let (start, end) = frames.unwrap_or((0, samples.len() as u32));
    let start = start.min(samples.len() as u32) as usize;
    let end = end.min(samples.len() as u32) as usize;
    let mut inputs = Vec::new();
    // Run-length encode into Hold segments.
    let mut i = start;
    while i < end {
        let s = &samples[i];
        let cur = (s.stick_x, s.stick_y, map_buttons(s.buttons_m64));
        let mut j = i + 1;
        while j < end {
            let t = &samples[j];
            if (t.stick_x, t.stick_y, map_buttons(t.buttons_m64)) != cur {
                break;
            }
            j += 1;
        }
        inputs.push(InputSeg::Hold {
            frames: (j - i) as u32,
            stick: (cur.0, cur.1),
            buttons: cur.2,
        });
        i = j;
    }
    Scenario {
        id: id.into(),
        tags: vec!["replay".into(), "m64".into()],
        geometry: vec![stepkit_trace::scenario::Geometry::Box(
            stepkit_trace::scenario::BoxGeom {
                min: (-2000.0, -100.0, -2000.0),
                max: (2000.0, 0.0, 2000.0),
                kind: 0,
            },
        )],
        start: StartState {
            pos: (0.0, 0.0, 0.0),
            yaw_deg: 0.0,
            action: "Idle".into(),
        },
        camera: Camera::Fixed { yaw_deg: 0.0 },
        inputs,
        compare: CompareProfile::GroundDefault,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_m64() -> Vec<u8> {
        let mut data = vec![0u8; 0x400 + 8];
        data[0..4].copy_from_slice(b"M64\x1A");
        data[4..8].copy_from_slice(&3u32.to_le_bytes());
        data[0x0C..0x10].copy_from_slice(&2u32.to_le_bytes());
        data[0x15] = 1;
        data[0x18..0x1C].copy_from_slice(&2u32.to_le_bytes());
        // frame 0: A pressed, stick (0, 80) -- stored Y inverted
        data[0x400..0x404].copy_from_slice(&[0x00, 0x80, 0x00, 0xB0]);
        // frame 1: nothing
        data[0x404..0x408].copy_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        data
    }

    #[test]
    fn parses_samples() {
        let (frames, samples) = parse_m64(&fake_m64()).unwrap();
        assert_eq!(frames, 2);
        assert_eq!(samples.len(), 2);
        assert_eq!(map_buttons(samples[0].buttons_m64), 0x0001);
        assert_eq!(samples[0].stick_y, 80); // un-inverted
        assert_eq!(samples[1].stick_x, 0);
    }

    #[test]
    fn rejects_bad_signature() {
        assert!(parse_m64(b"nope").is_err());
    }

    #[test]
    fn scenario_run_length_encodes() {
        let (_, samples) = parse_m64(&fake_m64()).unwrap();
        let s = samples_to_scenario("t", &samples, None);
        assert_eq!(s.inputs.len(), 2);
        assert_eq!(s.frame_count(), 2);
    }
}
