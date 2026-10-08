# Action: Wall Kick (ID 0x018808B0)

## Entry conditions
- A pressed while airborne with the wall-kick window open (10 frames after
  a wall hit). The ID is verified (wiki:Wall Kick rev 19311); the wiki's
  behavior page is currently a broken redirect, so all numbers are (verify).

## Per-frame order of checks
1. Jump-height control: no (verify).
2. B pressed with speed > 28 -> Dive (verify). Z pressed -> Ground Pound.
3. Gravity -4, terminal -75; air step, can chain further wall kicks.

## Movement
- Entry: facing = away from wall, forward speed = 20 (verify),
  vertical speed = 52 (verify).
- Air steering 0x800/frame (verify).

## Transitions
| Condition | Next action |
| --- | --- |
| Landing | Landing (land_from = WALL_KICK) |
| Wall hit, A | Wall Kick (chain) |
| B, speed > 28 | Dive |
| Z | Ground Pound |

## Constants
`wall_kick.vertical` (verify), `wall_kick.forward` (verify),
`wall.kick_window` (verify)

## Animation needs
Slot `wall_kick` (phase 6).

## Evidence
- ID: wiki:Wall Kick rev 19311 (behavior page unreachable 2026-10-08).
- Scenario `wall_kick` (Jump -> WallKick -> Land).

## Open questions
- All entry/behavior numbers -> probe/oracle when the wiki page returns.
