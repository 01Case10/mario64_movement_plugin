# Action: Butt Slide (ID 0x40000447 PROVISIONAL, group moving)

> No community page documents Butt Slide's hex/id; the ID is
> stepkit-provisional (bit 30 set). All numbers (verify).

## Entry conditions
- On the ground with a Slide-kind floor (Walking, Braking, Decelerating
  check each tick). Steep-slope auto-slide is not in v1 (open question).

## Per-frame order of checks
1. Floor query -> Freefall if no floor.
2. A pressed -> Single Jump (verify).
3. Left the slide surface -> Walking (stick) or Decelerating.

## Movement
- Downhill acceleration 2.5 * steepness along the slope (verify);
  friction -1.0/frame on flat (verify); cap 48.
- Steering: facing approaches intended yaw (or downhill) at 0x400/frame.
- Below speed 2.0 -> get up (Walking/Idle).

## Constants
`slide.downhill_accel` (verify), `slide.friction` (verify),
`slide.steer_rate` (verify), `slide.min_speed` (verify)

## Animation needs
Slot `butt_slide` (phase 6).

## Evidence
- Structure only; scenario `slide`. Numbers await probes/oracle.

## Open questions
- All numbers; steep-slope (non-Slide-kind) auto-slide threshold.
