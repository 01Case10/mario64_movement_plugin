# Action: Triple Jump (ID 0x01000882)

## Entry conditions
- A pressed during Landing when the landing came from a Double Jump AND
  forward speed > 20.
- Entry: vertical speed = 69 (fixed, independent of horizontal speed);
  forward speed *= 0.8. No jump-height control.
  (wiki:Triple Jump@19300)

## Per-frame order of checks
1. No height control (documented).
2. B pressed -> Dive. Z pressed -> Ground Pound.
3. Gravity -4, terminal -75; air step WITHOUT ledge-grab/hang flags
   (documented; phase 4).

## Movement
- Air steering 0x800/frame (verify).

## Transitions
| Condition | Next action |
| --- | --- |
| Landing | Landing (land_from = TRIPLE_JUMP) |
| B | Dive |
| Z | Ground Pound |

## Constants
`jump.triple_vertical`, `jump.forward_retain`

## Animation needs
Slot `triple_jump` (phase 6).

## Evidence
- ID, entry, no-height-control, no-ledge-grab: wiki:Triple Jump@19300
  (rev 2024-06-18).
- Chain rule: same page (double jump land + A + speed > 20).
- L1 test `anchor_triple_jump_entry`; scenario `jump_chain_triple`
  (Jump -> Land -> DblJ -> Land -> TrpJ).

## Open questions
- The "double jump timer" expiring while walking after a double-jump land
  (wiki lists Walking & Decelerating chain conditions) -> phase 4.
