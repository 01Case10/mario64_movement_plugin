# Action: Walking (ID 0x40000440 PROVISIONAL, group moving)

> The community wiki page for Walking lists its hex/id as "todo", so the ID
> above is stepkit-provisional (bit 30 set) and will be replaced when the
> community documents the real one.

> **No separate "running" action.** The community documentation describes a
> single stick-held locomotion action (Walking) covering all speeds, with the
> target speed capped at 32. The plan's separate "running" action was a
> working assumption; the evidence supports one action, so this
> implementation has one. High-speed locomotion selects different animation
> clips by speed (phase 6) but stays in Walking.

## Entry conditions
- Stick held while in Idle, Braking, Decelerating, Landing, or TurningAround
  (when re-aligned).
- Entry clamp: if the floor is not very slippery and current forward speed
  is below min(intended stick magnitude, 8), speed is set to
  min(intended stick magnitude, 8). (wiki:Walking@19299)

## Per-frame order of checks
1. Floor query: no floor within the step-down snap -> Freefall.
2. A pressed -> Single Jump.
3. (B dive: documented on the Walking page; implemented with the Dive action
   in phase 4.)
4. Stick intended yaw more than 90 deg from facing and speed >= 16
   -> Turning Around.
5. Stick not held: speed >= 16 -> Braking; speed < 16 -> Decelerating.

## Movement
- Target speed = min(intended stick magnitude, 32) (24 on slow floors;
  slow floors not modeled in v1).
- If speed <= 0: speed += 1.1 per frame.
- Else if speed <= target: speed += (1.1 - speed/43) per frame.
- Else if speed > target and floor normal.y >= 0.95: speed -= 1.0 per frame.
- Speed is hard-capped at 48.
- Slope: speed is adjusted downhill/uphill, scaled by steepness
  (1 - floor normal.y), by floor class: 5.3 very slippery, 2.7 slippery,
  1.7 default, 0.0 non-slippery. (The exact steepness scaling is (verify).)
- Facing approaches the intended stick yaw at up to 0x800 angle units/frame.
- Position advances by facing * speed through the ground quarter-step.

## Transitions
| Condition | Next action |
| --- | --- |
| No floor underfoot | Freefall |
| A pressed | Single Jump |
| Stick back (>90 deg) and speed >= 16 | Turning Around |
| Stick released and speed >= 16 | Braking |
| Stick released and speed < 16 | Decelerating |

## Constants
`walk.target_speed_cap`, `walk.speedup_base`, `walk.speedup_falloff_divisor`,
`walk.decel_flat_ground`, `walk.hard_cap`, `walk.turn_rate`,
`walk.enter_speed_clamp`, `walk.turnaround_speed`, `walk.brake_speed`,
`slope.accel_default`, `slope.accel_slippery`, `slope.accel_very_slippery`,
`slope.accel_not_slippery`

## Animation needs
Slot `walk_cycle` (loop), speed scaled by forward speed (phase 6 manifest).
Footstep events at the manifest's frames.

## Evidence
- wiki:Walking@19299 (rev 2024-06-18): target speed cap, accel curve,
  decel, hard cap, turn rate, entry clamp, turnaround/brake thresholds,
  slope adjustments, quicksand rule.
- Anchors: traces/public-anchors/walking_behavior.csv.
- L1 test `anchor_walk_acceleration_curve` reproduces the entry clamp, the
  1.1 - speed/43 curve, the 32 target, the 48 cap, and the -1.0 decel cycle.

## Open questions
- Exact steepness scaling of the slope adjustment -> probe scenario family
  (phase 4).
- Slow-floor (24) target -> needs a slow surface kind in scenarios.
