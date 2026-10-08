# Action: Ledge Grab (ID 0x0800034B)

## Entry conditions
Checked during the air step when a quarter step hits a wall, for actions
with the ledge-grab flag (Single/Double Jump, Side Flip, Long Jump, Dive,
Freefall, Wall Kick -- verify for Dive/Freefall/Wall Kick). Triple Jump and
Backflip explicitly cannot grab (documented).
1. Vertical velocity <= 0.
2. Wall 30 units above Mario (probe with Mario's radius).
3. NO wall 150 units above Mario.
4. Top-down floor search from 60 units into the wall and 160 units above
   finds a floor.
(wiki:Ledge Grab@19518)

## Behavior (v1 simplification)
- Snap to the found floor at the search xz (10 units into the wall, per the
  wiki), face the wall, zero velocity.
- Hang timer; A -> climb (stand up in place -> Idle); Z -> let go (drop
  away: forward = -8 -> Freefall); timer 10 -> auto-climb.
- The wiki's slow-climb variants and the 60-backdrop are not in v1.

## Transitions
| Condition | Next action |
| --- | --- |
| A pressed | Idle (on the ledge) |
| Z pressed | Freefall (drop) |
| Timer 10 | Idle (auto-climb) |

## Constants
`ledge.wall_check_low`, `ledge.wall_check_high`, `ledge.search_inward`,
`ledge.search_up`, `ledge.drop_speed`, `ledge.hang_frames`

## Animation needs
Slot `ledge_grab`, `ledge_climb` (phase 6).

## Evidence
- wiki:Ledge Grab@19518 (rev 2024-06-18): conditions, search geometry,
  30-150 raise range, climb/drop rules.
- L1 test `anchor_ledge_grab_conditions`; scenario `ledge_grab`.

## Open questions
- Slow-climb variants, exact hang timing -> phase 5+.
