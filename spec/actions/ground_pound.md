# Action: Ground Pound (ID 0x400008A8 PROVISIONAL, group airborne)

> No community page documents Ground Pound's hex/id; the ID is
> stepkit-provisional (bit 30 set).

## Entry conditions
- Z pressed while airborne (any jump, flip, dive, freefall).

## Behavior
1. Entry kills momentum: forward = 0, vy = 0.
2. Hover 8 frames (verify), then slam: gravity -4 applies from vy = 0
   (verify), no steering.
3. Landing -> Landing (land_from = GROUND_POUND).

## Constants
`ground_pound.hover_frames` (verify), `physics.gravity`

## Animation needs
Slot `ground_pound` (phase 6).

## Evidence
- Structure follows the documented air-cancel pattern (Z in air);
  all numbers (verify). Scenario `ground_pound`.

## Open questions
- Hover duration, slam speed, landing recovery -> probe/oracle.
