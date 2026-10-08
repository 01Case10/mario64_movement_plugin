# Action: Air Knockback (ID 0x400008B1 PROVISIONAL, group airborne)

> No community page documents this hex/id; the ID is stepkit-provisional
> (bit 30 set). The dive->bonk->knockback chain is documented
> (wiki:Dive@19303); all numbers (verify).

## Entry conditions
- Wall hit while in Dive.

## Per-frame order of checks
1. No steering (stunned), no dive/pound cancels, no ledge grab.
2. Gravity -4, terminal -75.

## Movement
- Entry: facing = away from wall, forward = -15 (backward), vy = 40.

## Transitions
| Condition | Next action |
| --- | --- |
| Landing | Landing (land_from = AIR_KNOCKBACK) |

## Constants
`knockback.vertical` (verify), `knockback.backward_speed` (verify)

## Animation needs
Slot `air_knockback` (phase 6).

## Evidence
- Chain documented: wiki:Dive@19303 ("Backwards Air Knockback by bonking").
- Scenario `dive_bonk`.

## Open questions
- All numbers; whether other air actions bonk -> probe.
