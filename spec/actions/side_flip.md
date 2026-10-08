# Action: Side Flip (ID 0x01000887)

## Entry conditions
- A pressed during Turning Around.
- Entry: vertical speed = 62, forward speed = 8, facing = intended stick yaw.
  (wiki:Side Flip@20374)

## Per-frame order of checks
1. B pressed -> Dive. Z pressed -> Ground Pound.
2. Gravity -4, terminal -75; air step WITH ledge grab (documented; phase 4).

## Movement
- Air steering 0x800/frame (verify).

## Transitions
| Condition | Next action |
| --- | --- |
| Landing | Landing (land_from = SIDE_FLIP; chains to double jump) |
| B | Dive |
| Z | Ground Pound |

## Constants
`jump.sideflip_vertical`, `jump.sideflip_forward`

## Animation needs
Slot `side_flip` (phase 6).

## Evidence
- ID, entry, ledge-grab allowed, double-jump-after-landing:
  wiki:Side Flip@20374 (rev 2026-05-25).
- L1 test `anchor_side_flip_entry`; scenario `sideflip`.

## Open questions
- Height control on side flip -> probe.
