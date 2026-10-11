//! Reader for the native oracle's CSV output (plan §6 Phase 2, ST-02).
//!
//! The schema is the 49-column layout the oracle writes (§7.3 of the plan,
//! `CSV_HEADER` in `oracle/harness.c`). Floats are printed with `%.9g`,
//! which round-trips `f32` through `f64`; action-like fields are `0x%08X`.
//!
//! This is the parse side of the future `stepkit oracle diff`; the diff
//! itself is a later Phase 2 item.

/// The 49 oracle CSV column names, in order. The header line must match
/// these exactly.
pub const ORACLE_COLUMNS: &[&str] = &[
    "frame",
    "stick_x",
    "stick_y",
    "buttons",
    "cam_yaw",
    "pos_x",
    "pos_y",
    "pos_z",
    "vel_x",
    "vel_y",
    "vel_z",
    "forward_vel",
    "slide_vel_x",
    "slide_vel_z",
    "slide_yaw",
    "face_pitch",
    "face_yaw",
    "face_roll",
    "angle_vel_x",
    "angle_vel_y",
    "angle_vel_z",
    "intended_yaw",
    "intended_mag",
    "action",
    "prev_action",
    "action_state",
    "action_timer",
    "action_arg",
    "input",
    "floor_height",
    "floor_angle",
    "floor_normal_y",
    "floor_type",
    "ceil_height",
    "wall_normal_yaw",
    "water_level",
    "wall_kick_timer",
    "double_jump_timer",
    "squish_timer",
    "quicksand_depth",
    "peak_height",
    "health",
    "hurt_counter",
    "invinc_timer",
    "flags",
    "anim_id",
    "anim_frame",
    "anim_accel",
    "anim_frame_accel_assist",
];

/// One parsed oracle CSV row, with typed fields.
#[derive(Clone, Debug, PartialEq)]
pub struct OracleFrame {
    pub frame: u32,
    /// Adjusted stick (`%.9g`).
    pub stick_x: f32,
    pub stick_y: f32,
    pub buttons: u32,
    pub cam_yaw: f32,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub vel_x: f32,
    pub vel_y: f32,
    pub vel_z: f32,
    pub forward_vel: f32,
    pub slide_vel_x: f32,
    pub slide_vel_z: f32,
    pub slide_yaw: i32,
    pub face_pitch: i16,
    pub face_yaw: i16,
    pub face_roll: i16,
    pub angle_vel_x: i16,
    pub angle_vel_y: i16,
    pub angle_vel_z: i16,
    pub intended_yaw: i16,
    pub intended_mag: f32,
    pub action: u32,
    pub prev_action: u32,
    pub action_state: u32,
    pub action_timer: u32,
    pub action_arg: u32,
    pub input: u32,
    pub floor_height: f32,
    pub floor_angle: i16,
    pub floor_normal_y: f32,
    pub floor_type: i32,
    pub ceil_height: f32,
    /// `None` when the oracle prints `none` (no wall hit).
    pub wall_normal_yaw: Option<i32>,
    pub water_level: i32,
    pub wall_kick_timer: i32,
    pub double_jump_timer: i32,
    pub squish_timer: i32,
    pub quicksand_depth: f32,
    pub peak_height: f32,
    pub health: i32,
    pub hurt_counter: i32,
    pub invinc_timer: i32,
    pub flags: u32,
    pub anim_id: i32,
    pub anim_frame: i32,
    pub anim_accel: i32,
    pub anim_frame_accel_assist: i32,
}

fn parse_f32(s: &str, col: &str, line: usize) -> Result<f32, String> {
    // %.9g round-trips f32: parse as f64 first, then narrow.
    s.parse::<f64>()
        .map(|v| v as f32)
        .map_err(|_| format!("line {line}: column {col}: {s:?} is not a float"))
}

fn parse_i32(s: &str, col: &str, line: usize) -> Result<i32, String> {
    s.parse::<i32>()
        .map_err(|_| format!("line {line}: column {col}: {s:?} is not an integer"))
}

fn parse_u32(s: &str, col: &str, line: usize) -> Result<u32, String> {
    s.parse::<u32>()
        .map_err(|_| format!("line {line}: column {col}: {s:?} is not an unsigned integer"))
}

fn parse_hex(s: &str, col: &str, line: usize) -> Result<u32, String> {
    let digits = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .ok_or_else(|| {
            format!("line {line}: column {col}: {s:?} is not a 0x-prefixed hex value")
        })?;
    u32::from_str_radix(digits, 16)
        .map_err(|_| format!("line {line}: column {col}: {s:?} is not hex"))
}

fn parse_i16(s: &str, col: &str, line: usize) -> Result<i16, String> {
    s.parse::<i16>()
        .map_err(|_| format!("line {line}: column {col}: {s:?} is not an integer"))
}

/// Parse oracle CSV text into frames. The header must match
/// [`ORACLE_COLUMNS`] exactly; every error names its line number.
pub fn read_oracle_csv(text: &str) -> Result<Vec<OracleFrame>, String> {
    let mut lines = text.lines();
    let header = lines.next().ok_or("oracle csv: empty input")?;
    let cols: Vec<&str> = header.split(',').collect();
    if cols != ORACLE_COLUMNS {
        return Err(format!(
            "oracle csv: header mismatch: expected {} columns starting {:?}, got {} columns starting {:?}",
            ORACLE_COLUMNS.len(),
            &ORACLE_COLUMNS[..3],
            cols.len(),
            &cols[..cols.len().min(3)],
        ));
    }
    let mut out = Vec::new();
    for (i, line) in lines.enumerate() {
        let line_no = i + 2; // 1-based, after the header
        if line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split(',').collect();
        if f.len() != ORACLE_COLUMNS.len() {
            return Err(format!(
                "line {line_no}: expected {} fields, got {}",
                ORACLE_COLUMNS.len(),
                f.len()
            ));
        }
        let g = |idx: usize| -> Result<&str, String> {
            f.get(idx)
                .copied()
                .ok_or_else(|| format!("line {line_no}: missing column {}", ORACLE_COLUMNS[idx]))
        };
        let wall_normal_yaw = {
            let s = g(34)?;
            if s == "none" {
                None
            } else {
                Some(parse_i32(s, "wall_normal_yaw", line_no)?)
            }
        };
        out.push(OracleFrame {
            frame: parse_u32(g(0)?, "frame", line_no)?,
            stick_x: parse_f32(g(1)?, "stick_x", line_no)?,
            stick_y: parse_f32(g(2)?, "stick_y", line_no)?,
            buttons: parse_u32(g(3)?, "buttons", line_no)?,
            cam_yaw: parse_f32(g(4)?, "cam_yaw", line_no)?,
            pos_x: parse_f32(g(5)?, "pos_x", line_no)?,
            pos_y: parse_f32(g(6)?, "pos_y", line_no)?,
            pos_z: parse_f32(g(7)?, "pos_z", line_no)?,
            vel_x: parse_f32(g(8)?, "vel_x", line_no)?,
            vel_y: parse_f32(g(9)?, "vel_y", line_no)?,
            vel_z: parse_f32(g(10)?, "vel_z", line_no)?,
            forward_vel: parse_f32(g(11)?, "forward_vel", line_no)?,
            slide_vel_x: parse_f32(g(12)?, "slide_vel_x", line_no)?,
            slide_vel_z: parse_f32(g(13)?, "slide_vel_z", line_no)?,
            slide_yaw: parse_i32(g(14)?, "slide_yaw", line_no)?,
            face_pitch: parse_i16(g(15)?, "face_pitch", line_no)?,
            face_yaw: parse_i16(g(16)?, "face_yaw", line_no)?,
            face_roll: parse_i16(g(17)?, "face_roll", line_no)?,
            angle_vel_x: parse_i16(g(18)?, "angle_vel_x", line_no)?,
            angle_vel_y: parse_i16(g(19)?, "angle_vel_y", line_no)?,
            angle_vel_z: parse_i16(g(20)?, "angle_vel_z", line_no)?,
            intended_yaw: parse_i16(g(21)?, "intended_yaw", line_no)?,
            intended_mag: parse_f32(g(22)?, "intended_mag", line_no)?,
            action: parse_hex(g(23)?, "action", line_no)?,
            prev_action: parse_hex(g(24)?, "prev_action", line_no)?,
            action_state: parse_u32(g(25)?, "action_state", line_no)?,
            action_timer: parse_u32(g(26)?, "action_timer", line_no)?,
            action_arg: parse_hex(g(27)?, "action_arg", line_no)?,
            input: parse_hex(g(28)?, "input", line_no)?,
            floor_height: parse_f32(g(29)?, "floor_height", line_no)?,
            floor_angle: parse_i16(g(30)?, "floor_angle", line_no)?,
            floor_normal_y: parse_f32(g(31)?, "floor_normal_y", line_no)?,
            floor_type: parse_i32(g(32)?, "floor_type", line_no)?,
            ceil_height: parse_f32(g(33)?, "ceil_height", line_no)?,
            wall_normal_yaw,
            water_level: parse_i32(g(35)?, "water_level", line_no)?,
            wall_kick_timer: parse_i32(g(36)?, "wall_kick_timer", line_no)?,
            double_jump_timer: parse_i32(g(37)?, "double_jump_timer", line_no)?,
            squish_timer: parse_i32(g(38)?, "squish_timer", line_no)?,
            quicksand_depth: parse_f32(g(39)?, "quicksand_depth", line_no)?,
            peak_height: parse_f32(g(40)?, "peak_height", line_no)?,
            health: parse_i32(g(41)?, "health", line_no)?,
            hurt_counter: parse_i32(g(42)?, "hurt_counter", line_no)?,
            invinc_timer: parse_i32(g(43)?, "invinc_timer", line_no)?,
            flags: parse_hex(g(44)?, "flags", line_no)?,
            anim_id: parse_i32(g(45)?, "anim_id", line_no)?,
            anim_frame: parse_i32(g(46)?, "anim_frame", line_no)?,
            anim_accel: parse_i32(g(47)?, "anim_accel", line_no)?,
            anim_frame_accel_assist: parse_i32(g(48)?, "anim_frame_accel_assist", line_no)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> String {
        ORACLE_COLUMNS.join(",")
    }

    fn sample_csv() -> String {
        let mut s = header();
        s.push('\n');
        // Row 0: standing idle, no wall.
        s.push_str("0,0,0,0,180,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0x0C400201,0x0C400201,0,0,0x00000000,0x00000000,0,0,1,0,0,none,0,0,0,0,0,0,256,0,0,0x00000000,0,0,0,0\n");
        // Row 1: jumping with a wall hit.
        s.push_str("1,12,-80,32768,180.5,100.5,200.25,-50.75,10.5,42,-5.25,22.5,1.5,-1.5,4096,100,-200,300,5,-5,10,8192,31.75,0x0100088C,0x0C400201,2,15,0x00000001,0x00000100,150.5,1234,0.848,20,400.5,5678,-1500,5,3,8,2.5,300.75,200,4,30,0x00000008,42,7,3,1\n");
        s
    }

    #[test]
    fn columns_match_harness_header() {
        assert_eq!(ORACLE_COLUMNS.len(), 49);
        assert_eq!(ORACLE_COLUMNS[0], "frame");
        assert_eq!(ORACLE_COLUMNS[48], "anim_frame_accel_assist");
    }

    #[test]
    fn parses_two_rows() {
        let frames = read_oracle_csv(&sample_csv()).unwrap();
        assert_eq!(frames.len(), 2);

        let f0 = &frames[0];
        assert_eq!(f0.frame, 0);
        assert_eq!(f0.action, 0x0C400201);
        assert_eq!(f0.prev_action, 0x0C400201);
        assert_eq!(f0.wall_normal_yaw, None);
        assert_eq!(f0.health, 256);
        assert_eq!(f0.floor_type, 0);
        assert_eq!(f0.buttons, 0);

        let f1 = &frames[1];
        assert_eq!(f1.frame, 1);
        assert_eq!(f1.stick_x, 12.0);
        assert_eq!(f1.stick_y, -80.0);
        assert_eq!(f1.buttons, 32768);
        assert!((f1.cam_yaw - 180.5).abs() < 1e-6, "{}", f1.cam_yaw);
        assert!((f1.pos_y - 200.25).abs() < 1e-6, "{}", f1.pos_y);
        assert_eq!(f1.action, 0x0100088C);
        assert_eq!(f1.action_arg, 1);
        assert_eq!(f1.input, 0x100);
        assert_eq!(f1.wall_normal_yaw, Some(5678));
        assert!(
            (f1.floor_normal_y - 0.848).abs() < 1e-6,
            "{}",
            f1.floor_normal_y
        );
        assert_eq!(f1.water_level, -1500);
        assert_eq!(f1.flags, 8);
        assert_eq!(f1.anim_id, 42);
        assert_eq!(f1.anim_frame_accel_assist, 1);
        assert_eq!(f1.face_yaw, -200);
        assert_eq!(f1.floor_angle, 1234);
    }

    #[test]
    fn header_mismatch_errors() {
        let bad = "frame,stick_x\n0,0\n";
        let err = read_oracle_csv(bad).unwrap_err();
        assert!(err.contains("header mismatch"), "{err}");
    }

    #[test]
    fn bad_value_names_line_number() {
        let base = sample_csv();
        // A 49-field row whose stick_x is not a float, on line 4.
        let mut row: Vec<&str> = vec!["2", "not_a_float"];
        while row.len() < 49 {
            row.push("0");
        }
        let csv = format!("{}{}", base, row.join(","));
        let err = read_oracle_csv(&csv).unwrap_err();
        assert!(err.contains("line 4"), "{err}");
        assert!(err.contains("stick_x"), "{err}");
    }

    #[test]
    fn bad_hex_names_line_number() {
        let base = sample_csv();
        let mut lines: Vec<String> = base.lines().map(str::to_string).collect();
        let mut row2: Vec<&str> = lines[2].split(',').collect();
        row2[23] = "xyz"; // action
        lines[2] = row2.join(",");
        let err = read_oracle_csv(&lines.join("\n")).unwrap_err();
        assert!(err.contains("line 3"), "{err}");
        assert!(err.contains("action"), "{err}");
    }

    #[test]
    fn short_row_errors() {
        let mut s = header();
        s.push_str("\n0,0,0\n");
        let err = read_oracle_csv(&s).unwrap_err();
        assert!(err.contains("line 2"), "{err}");
        assert!(err.contains("49 fields"), "{err}");
    }
}
