# Action: Dive (ID 0x0188088A)

## Entry conditions
- Ground (Walking, Decelerating, Crawling): B pressed with speed >= 29
  and stick magnitude > 48 -> +15 horizontal (clamped to 48), vy = +20.
- Air (Single/Double/Triple Jump, Side Flip, Freefall): B pressed with
  speed > 28 -> +15 horizontal (clamped to 48), vy unchanged.
  (wiki:Dive@19303)

## Per-frame order of checks
1. Gravity -4 (verify), terminal -75. No height control (verify).
2. No air steering: committed (verify).
3. Air step; landing -> Landing (dive slide is phase 4).

## Movement
- Forward speed fixed at entry value; falls ballistically.

## Transitions
| Condition | Next action |
| --- | --- |
| Landing | Landing (land_from = DIVE) |

## Constants
`dive.horizontal_gain`, `dive.horizontal_cap`, `dive.vertical_gain_ground`,
`dive.ground_speed_threshold`, `dive.ground_stick_threshold`,
`dive.air_speed_threshold`

## Animation needs
Slot `dive` (phase 6).

## Evidence
- ID, thresholds, gains: wiki:Dive@19303 (rev 2024-06-18).
- L1 test `anchor_dive_entry_numbers`; scenarios `dive_ground`, `dive_air`.

## Open questions
- Gravity/steering in dive; dive-slide on landing -> phase 4.
