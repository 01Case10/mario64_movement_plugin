# Action: Braking (ID 0x40000444 PROVISIONAL, group moving)

> No community page documents Braking's hex/id; the ID is stepkit-provisional
> (bit 30 set).

## Entry conditions
- From Walking or TurningAround: stick released (neutral) while forward
  speed >= 16. (wiki:Walking@19299)

## Per-frame order of checks
1. Floor query: no floor within the step-down snap -> Freefall.
2. A pressed -> Single Jump.
3. Stick held -> Walking (with entry clamp).

## Movement
- Forward speed decreases by 4.0 per frame (verify -- rate not documented).
- At speed 0 -> Idle.

## Transitions
| Condition | Next action |
| --- | --- |
| No floor underfoot | Freefall |
| A pressed | Single Jump |
| Stick held | Walking |
| Speed reaches 0 | Idle |

## Constants
`walk.brake_speed`, `actions.braking.decel` (verify)

## Animation needs
Slot `brake` (phase 6).

## Evidence
- Entry condition: wiki:Walking@19299 ("neutral stick ... with speed >= 16
  brakes").

## Open questions
- Deceleration rate and any slide/dust behavior -> probe scenarios.
