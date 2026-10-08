# Action: Crawl (ID 0x40000011 PROVISIONAL, group moving)

> No community page documents Crawl's hex/id; the ID is stepkit-provisional
> (bit 30 set). All movement numbers (verify).

## Entry conditions
- Stick held while Crouching.

## Per-frame order of checks
1. Floor query -> Freefall if no floor.
2. Z released -> Idle.
3. A pressed -> Single Jump (verify).
4. No stick -> Crouch.

## Movement
- Target speed = min(stick magnitude, 10); approaches at 2.0/frame.
- Facing approaches intended yaw at 0x800/frame.

## Constants
`crawl.target_speed` (verify), `crawl.accel` (verify)

## Animation needs
Slot `crawl` (phase 6).

## Evidence
- Entry from crouch: wiki:Crouching@17805. Numbers (verify).

## Open questions
- Speed/accel and A behavior -> probe/oracle.
