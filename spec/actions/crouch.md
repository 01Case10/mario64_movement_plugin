# Action: Crouch (ID 0x40000010 PROVISIONAL, group stationary)

> No community page documents Crouch's hex/id; the ID is stepkit-provisional
> (bit 30 set).

## Entry conditions
- Z pressed while on the ground (Idle, Walking, Braking, Decelerating).
  (wiki:Crouching@17805)

## Per-frame order of checks
1. Floor query -> Freefall if no floor.
2. A pressed -> Backflip.
3. Z released -> stop crouching (Idle, or Walking if stick held).
4. Stick held -> Crawl.

## Movement
- Speed zeroed on entry; stationary.

## Transitions
| Condition | Next action |
| --- | --- |
| No floor | Freefall |
| A | Backflip |
| Z released, stick | Walking |
| Z released, no stick | Idle |
| Stick held | Crawl |

## Constants
None (behavioral).

## Animation needs
Slot `crouch` (phase 6).

## Evidence
- wiki:Crouching@17805 (rev 2023-03-07). Scenario `backflip`.

## Open questions
- None for v1.
