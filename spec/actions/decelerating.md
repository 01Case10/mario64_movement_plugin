# Action: Decelerating (ID 0x40000445 PROVISIONAL, group moving)

> No community page documents Decelerating's hex/id; the ID is
> stepkit-provisional (bit 30 set).

## Entry conditions
- From Walking or TurningAround: stick released (neutral) while forward
  speed < 16. (wiki:Walking@19299)

## Per-frame order of checks
1. Floor query: no floor within the step-down snap -> Freefall.
2. A pressed -> Single Jump.
3. Stick held -> Walking (with entry clamp).

## Movement
- Forward speed decreases by 0.5 per frame (verify -- rate not documented).
- At speed 0 -> Idle.

## Transitions
| Condition | Next action |
| --- | --- |
| No floor underfoot | Freefall |
| A pressed | Single Jump |
| Stick held | Walking |
| Speed reaches 0 | Idle |

## Constants
`walk.brake_speed`, `actions.decelerating.decel` (verify)

## Animation needs
Slot `decel` (phase 6).

## Evidence
- Entry condition: wiki:Walking@19299 ("below that decelerates").

## Open questions
- Deceleration rate -> probe scenarios.
