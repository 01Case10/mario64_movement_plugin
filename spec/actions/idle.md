# Action: Idle (ID 0x0C400201, group stationary)

## Entry conditions
- On the ground with no stick input and speed at (or near) zero: from
  Braking/Decelerating when speed reaches 0, from Landing when its timer
  expires with no stick held.

## Per-frame order of checks
1. Floor query: no floor within the step-down snap -> Freefall.
2. A pressed -> Single Jump (jump entry: vertical speed 42 + forward/4).
3. Stick held -> Walking (with the walk entry clamp).

## Movement
None. Forward speed is zeroed; no step is taken.

## Transitions
| Condition | Next action |
| --- | --- |
| No floor underfoot | Freefall |
| A pressed | Single Jump |
| Stick held (magnitude > deadzone) | Walking |

## Constants
`collision.radius`, `collision.height`, `input.stick_deadzone`,
`step.ground_step_down`

## Animation needs
Slot `idle` (loop). No events.

## Evidence
- ID: wiki:Idle@20401 (community-documented action infobox).
- Transitions: wiki:Walking@19299 documents the Walking entry clamp and
  the jump-out conditions shared by ground actions.

## Open questions
- None for v1.
