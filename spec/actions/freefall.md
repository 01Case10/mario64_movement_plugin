# Action: Freefall (ID 0x4000088C PROVISIONAL, group airborne)

> No community page documents Freefall's hex/id; the ID is stepkit-provisional
> (bit 30 set). (Note: 0x01000882 is Triple Jump per wiki:Triple Jump@19300.)

## Entry conditions
- Walked off a ledge from any ground action (floor beyond the step-down snap).
- A jump that expires without landing (phase 3 jump variants).

## Per-frame order of checks
1. Gravity: vertical speed -= 4 per frame, clamped to terminal -75.
2. Air step: ceiling/wall/floor as in Single Jump.
3. No jump-height control in freefall.

## Movement
- Same air steering as Single Jump (0x800/frame, verify).
- Forward speed unchanged (verify).

## Transitions
| Condition | Next action |
| --- | --- |
| Landing (floor within 78 below) | Landing |

## Constants
`physics.gravity`, `physics.terminal_velocity`,
`step.air_landing_snap_window`, `step.ceiling_zero_vel_window`

## Animation needs
Slot `freefall` (loop, phase 6).

## Evidence
- Gravity/terminal: wiki:Gravity@20294.
- Step windows: wiki:Movement steps@19723.

## Open questions
- Whether A can re-jump from a ledge freefall -> probe (phase 3).
