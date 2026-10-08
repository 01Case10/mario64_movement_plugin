# Action: Turning Around (ID 0x00000443, group moving)

## Entry conditions
- From Walking: stick intended yaw more than 90 deg from facing ("stick
  held back") while forward speed >= 16.

## Per-frame order of checks
1. Floor query: no floor within the step-down snap -> Freefall.
2. A pressed -> Single Jump.
3. Stick not held -> Braking (speed >= 16) or Decelerating (speed < 16).

## Movement
- Facing approaches the intended stick yaw at up to 0x1000 angle units/frame
  (verify -- rate not documented on the community page).
- Forward speed is unchanged while turning (verify).
- When aligned within one walk turn step (0x800), -> Walking.

## Transitions
| Condition | Next action |
| --- | --- |
| No floor underfoot | Freefall |
| A pressed | Single Jump |
| Stick released, speed >= 16 | Braking |
| Stick released, speed < 16 | Decelerating |
| Facing aligned with intended yaw | Walking |

## Constants
`walk.turnaround_speed`, `walk.turn_rate`,
`actions.turning_around.turn_rate` (verify)

## Animation needs
Slot `turn` (loop or once, phase 6).

## Evidence
- ID: wiki:Turning Around@18182 (community-documented action infobox).
- Entry condition: wiki:Walking@19299 ("stick held back with speed >= 16
  turns Mario around").

## Open questions
- Turn rate and speed behavior during the turn -> probe scenarios.
