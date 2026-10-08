# Action: Water Plunge (ID 0x300022E2) / Swimming (PROVISIONAL)

## Water Plunge entry (verified)
When Mario's height is more than 100 units below the water surface:
- Forward velocity is divided by 4.
- Y velocity is divided by 2.
- Height is set to 100 units below the water surface.
- Action becomes Water Plunge.
(wiki:Water Plunge, rev 2023-07-27)

Water Plunge is brief (10 frames in v1) then transitions to Swimming.

## Swimming (working assumptions, verify)
- 3D movement: stick steers yaw and pitch.
- B: stroke (+3 forward, cap 8). The cap is informed by a documented 0xA
  challenge noting B-only swimming caps around speed 7.
- A: stronger stroke (+5 forward, cap 24).
- Water drag: 0.98/frame. Buoyancy damps vertical toward neutral.
- Near-surface + strong upward pitch + speed exits to Freefall.
- Leaving the water volume exits to Freefall.

## Constants
`water.plunge_depth`, `water.swim_stroke_b`, `water.swim_cap_b`,
`water.swim_stroke_a`, `water.swim_cap_a`, `water.drag` (all but plunge_depth verify)

## Animation needs
Slots `water_plunge`, `swimming` (phase 6+).

## Evidence
- wiki:Water Plunge (rev 2023-07-27): hex, entry conditions.
- L1 test `anchor_water_plunge_entry`; scenario `swim`.

## Open questions
- All swimming locomotion numbers; drowning; metal cap behavior.
