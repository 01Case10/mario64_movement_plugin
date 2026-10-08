# Action: Landing (ID 0x40000446 PROVISIONAL, group stationary)

> No community page documents Landing's hex/id; the ID is stepkit-provisional
> (bit 30 set).

## Entry conditions
- From any airborne action when the air step reports a landing. The action
  argument carries the fall speed at touchdown.

## Per-frame order of checks
1. Floor query: no floor within the step-down snap -> Freefall.
2. A pressed -> Single Jump (jump chaining; double/triple rules in phase 3).

## Movement
- Fixed 4-frame recovery (verify -- duration not documented). This window is
  the jump-chain timing: A during the window chains (see double_jump.md).
- Forward speed is preserved through the landing (the triple jump chain
  needs speed > 20 after a double-jump land); exact recovery (verify).
- After the timer: stick held -> Walking, else Decelerating.

## Transitions
| Condition | Next action |
| --- | --- |
| No floor underfoot | Freefall |
| A pressed | Single Jump |
| Timer expired, stick held | Walking |
| Timer expired, no stick | Decelerating |

## Constants
`actions.landing.duration_frames` (verify)

## Animation needs
Slot `land` (once, phase 6).

## Evidence
- Landing window (78 units): wiki:Movement steps@19723.

## Open questions
- Recovery duration vs fall speed; hard-landing variants -> probes.
