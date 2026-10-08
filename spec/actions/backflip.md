# Action: Backflip (ID 0x01000883)

## Entry conditions
- A pressed while Crouching (Z held on the ground).
- Entry: forward speed = -16, vertical speed = 62.
  (wiki:Backflip@19307)

## Per-frame order of checks
1. Z pressed -> Ground Pound.
2. Gravity -4, terminal -75; full air step.

## Movement
- Air steering 0x800/frame (holding forward can net forward movement --
  documented). Negative forward speed moves backward.

## Transitions
| Condition | Next action |
| --- | --- |
| Landing | Landing (land_from = BACKFLIP) |
| Z | Ground Pound |

## Constants
`jump.backflip_forward`, `jump.backflip_vertical`

## Animation needs
Slot `backflip` (phase 6).

## Evidence
- ID and entry: wiki:Backflip@19307 (rev 2024-06-18).
- L1 test `anchor_backflip_entry`; scenario `backflip`.

## Open questions
- Height control on backflip -> probe.
