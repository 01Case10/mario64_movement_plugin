# Action: Single Jump (ID 0x03000880, group airborne)

## Entry conditions
- A pressed in Idle, Walking, Braking, Decelerating, Landing, TurningAround.
- Entry: vertical speed = 42 + forward_speed/4; forward speed *= 0.8.
  (Vertical speed is halved if squished or in quicksand deeper than 1 --
  not modeled in v1.)

## Per-frame order of checks
1. Jump-height control: if A is not held and vertical speed > 20, vertical
   speed is quartered (the per-frame repeat is (verify)).
2. Gravity: vertical speed -= 4 per frame, clamped to terminal -75.
3. (B dive / ground pound: phase 3.)
4. Air step: ceiling within 160 above with upward velocity zeroes it;
   floor within 78 below while falling lands.

## Movement
- Air steering: facing approaches the intended stick yaw at up to 0x800
  angle units/frame (verify -- rate not documented).
- Forward speed is unchanged in the air (verify).
- Position advances by full velocity through the air quarter-step.

## Transitions
| Condition | Next action |
| --- | --- |
| Landing (floor within 78 below) | Landing |
| (Wall kick, dive, ground pound: phases 3-4) | |

## Constants
`jump.vertical_base`, `jump.vertical_forward_factor`,
`jump.forward_retain`, `jump.height_control_threshold`,
`physics.gravity`, `physics.terminal_velocity`,
`step.air_landing_snap_window`, `step.ceiling_zero_vel_window`

## Animation needs
Slot `jump_rise` / `jump_fall` (phase 6 manifest splits by vy sign).

## Evidence
- ID: wiki:Single Jump@19309 (community-documented action infobox).
- Entry numbers: wiki:Single Jump@19309 (42 + speed/4, x0.8).
- Gravity/terminal: wiki:Gravity@20294.
- Height control: wiki:Gravity@20294 ("vertical speed > 20 and A not held:
  vertical speed quartered").
- Step windows: wiki:Movement steps@19723.
- Anchors: traces/public-anchors/airborne_numbers.csv.
- L1 test `anchor_jump_entry_numbers` checks vy = 50 and speed 25.6 from
  a 32-speed jump.

## Open questions
- Exact repeat behavior of the height-control quartering -> probe.
- Air drag / steering rates -> probe scenarios (phase 3).
