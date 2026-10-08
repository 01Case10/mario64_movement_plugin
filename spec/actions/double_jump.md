# Action: Double Jump (ID 0x03000880)

## Entry conditions
- A pressed during Landing when the landing came from a Single Jump,
  Side Flip, or Freefall (land_from in {JUMP, SIDE_FLIP, FREEFALL}).
- Entry: vertical speed = 52 + forward_speed/4; forward speed *= 0.8.
  (wiki:Double Jump@18964)

## Per-frame order of checks
1. Jump-height control (vy > 20 and A not held -> vy quartered). (verify)
2. B pressed with speed > 28 -> Dive; otherwise no jump kick in v1 (note).
3. Z pressed -> Ground Pound.
4. Gravity -4, terminal -75; air step with ledge-grab + hang flags (phase 4).

## Movement
- Air steering 0x800/frame (verify). Forward speed unchanged in the air.

## Transitions
| Condition | Next action |
| --- | --- |
| Landing | Landing (land_from = DOUBLE_JUMP) |
| B, speed > 28 | Dive |
| Z | Ground Pound |

## Constants
`jump.double_vertical_base`, `jump.double_vertical_forward_factor`,
`jump.forward_retain`, `physics.gravity`, `physics.terminal_velocity`

## Animation needs
Slot `double_jump` (phase 6).

## Evidence
- ID and entry: wiki:Double Jump@18964 (rev 2024-01-21).
- Chain rule: same page ("entered by pressing A while in a Single Jump
  Land, a Sideflip Land, a Freefall Land").
- L1 test `anchor_double_jump_entry` (vy = 60 at speed 32) and
  `anchor_jump_chain` (single land + A -> double).

## Open questions
- Whether height control applies to the double jump -> probe.
