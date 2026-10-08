# Action: Long Jump (ID 0x03000888)

## Entry conditions
- A pressed while Z is held and moving on the ground with speed >= 16
  (threshold (verify)). The wiki describes it as "generally done by
  crouching and then pressing A" (wiki:Long Jump@14374).

## Per-frame order of checks
1. Gravity -2 (documented: wiki:Gravity@20294), terminal -75.
2. Z pressed -> Ground Pound (verify).
3. Air step with ledge-grab flag only (documented; phase 4).

## Movement
- Entry: vy = 28 (verify), forward += 18 (verify), unbounded by design.
- Air steering 0x800/frame (verify).

## Transitions
| Condition | Next action |
| --- | --- |
| Landing | Landing (land_from = LONG_JUMP) |
| B, speed > 28 | Dive |
| Z | Ground Pound |

## Constants
`jump.longjump_vertical` (verify), `jump.longjump_forward_gain` (verify),
`physics.longjump_gravity` (-2, documented)

## Animation needs
Slot `long_jump`; fast variant when entry speed > 16
(`longjump_fast_anim_threshold`, phase 6).

## Evidence
- ID: wiki:Long Jump@14374. Gravity -2: wiki:Gravity@20294.
- Scenario `long_jump`.

## Open questions
- Entry numbers and speed threshold -> probe/oracle.
