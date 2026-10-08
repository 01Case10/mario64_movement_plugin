# Ukikipedia provenance notes

Retrieved 2026-10-07 via the public ukikipedia.net MediaWiki API. This file
summarizes community-documented behavior in prose and tables only. It contains
no source code and no source-level identifiers; everything below is a
behavioral description plus measured/published numbers.

Citation convention: `wiki:<PageTitle>@<revision>` (revision ID, then the
revision's timestamp). Numeric anchors are transcribed as CSVs in
`../../traces/public-anchors/`.

## Pages fetched

| Page | Revision | Timestamp | Notes |
|---|---|---|---|
| Walking | 19299 | 2024-06-18T20:51:20Z | walking speed behavior, cancels, transition-in |
| Quarter Steps | 14388 | 2021-05-12T01:28:58Z | redirect; content lives on "Movement steps" |
| Movement steps | 19723 | 2025-08-19T16:29:37Z | quarter-step mechanics (ground/air) |
| Action | 19733 | 2025-08-19T20:17:25Z | action value format, 7 groups, flag table |
| Idle | 20401 | 2026-05-27T18:50:07Z | hex id for Idle |
| Single Jump | 19309 | 2024-06-19T14:32:18Z | hex id + jump entry behavior |
| Double Jump | 18964 | 2024-01-21T01:22:33Z | hex id |
| Triple Jump | 19300 | 2024-06-18T20:55:28Z | hex id |
| Backflip | 19307 | 2024-06-18T21:32:03Z | hex id |
| Side Flip | 20374 | 2026-05-25T01:18:56Z | hex id |
| Long Jump | 14374 | 2021-05-12T01:25:29Z | hex id + behavior |
| Dive | 19303 | 2024-06-18T21:14:27Z | hex id + dive entry behavior |
| Turning Around | 18182 | 2023-08-12T19:45:04Z | hex id |
| Ledge Grab | 19518 | 2025-01-12T20:30:38Z | hex id |
| Gravity | 20294 | 2026-05-24T07:16:07Z | per-action gravity and terminal velocities |
| De facto speed | 14131 | 2021-05-11T01:33:41Z | effective-speed definition |
| Wall Kick Air | 18139 | 2023-08-01T21:02:09Z | related action (see gaps below) |

## Action value format (wiki:Action@19733)

An action is a 32-bit value. The lowest 9 bits are a unique action ID ranging
from 0x000 to 0x192. There are 230 valid actions; some are only reachable by
hacking. IDs 0x050 and 0x051 are "pseudo-actions" (begin-sliding) that
immediately transition into a butt slide or stomach slide depending on whether
Mario faces uphill or downhill. ID 0x00E appears to have been defined but never
implemented. ID 0x000 is the "uninitialized" action and belongs to no group.

The remaining bits (except bit 30, which is unused) are action flags, 22 in
total. Bits 9–31 with community names: stationary(9), moving(10), airborne(11),
intangible(12), swimming(13), metal water(14), short hitbox(15), riding
shell(16), invulnerable(17), butt/stomach slide(18), diving(19), on pole(20),
hanging(21), idle(22), attacking(23), interruptable by wind(24), control jump
height(25), allow first person(26), pause exit(27), swimming/flying(28, unused),
water/text(29, unused), throwing(31). Spot-check: the Dive hex 0x0188088A has
bits 11 (air), 19 (diving), 23 (attacking), 24 (wind-interruptible) and 28
(unused) set, matching its documented flag list (Air, Diving, Attacking, Allow
Vertical Wind Action) plus one documented-unused bit — consistent with the
public flag table.

The seven action groups (ID ranges):

| Group | Range | Count | Description |
|---|---|---|---|
| stationary | 0x001–0x03F | 36 | stopped while on ground |
| moving | 0x040–0x07F | 40 | moving while on ground |
| airborne | 0x080–0x0BF | 45 | moving in air |
| submerged | 0x0C0–0x0FF | 32 | in water, including with metal cap |
| cutscene | 0x100–0x13F | 51 | no control of Mario |
| automatic | 0x140–0x17F | 17 | restricted control of Mario |
| object | 0x180–0x1BF | 10 | picking up or releasing objects |

## Verified action hex IDs (wiki action-page infoboxes)

| Action | Hex | ID (low 9 bits) | Group |
|---|---|---|---|
| Idle | 0x0C400201 | 0x001 | Stationary |
| Turning Around | 0x00000443 | 0x043 | Moving |
| Single Jump | 0x03000880 | 0x080 | Airborne |
| Double Jump | 0x03000881 | 0x081 | Airborne |
| Triple Jump | 0x01000882 | 0x082 | Airborne |
| Backflip | 0x01000883 | 0x083 | Airborne |
| Side Flip | 0x01000887 | 0x087 | Airborne |
| Long Jump | 0x03000888 | 0x088 | Airborne |
| Dive | 0x0188088A | 0x08A | Airborne |
| Ledge Grab | 0x0800034B | 0x14B | Automatic |

Not documented on ukikipedia (no standalone pages; "Wall kick" and "Ground
pound" are broken redirects to missing pages, and "Braking", "Decelerating",
"Landing", "Freefall" do not exist as pages): Walking (infobox lists "todo"
for both hex and id), Braking, Decelerating, Landing, Freefall, Ground Pound,
Wall Kick. A related action "Wall Kick Air" (rev 18139, 2023-08-01) exists but
was not one of the requested IDs, so its hex was not transcribed.

## Walking (wiki:Walking@19299)

Walking is the action for moving with the analog stick alone. On entry, if the
floor is not very slippery and the current forward speed is between 0 and the
smaller of the intended stick magnitude and 8, the speed is set to that
smaller value (so Mario never enters walking faster than 8 unless he was
already faster).

Speed update, per frame, on flat ground with a fully held stick:

- Target speed is the intended stick magnitude, capped at 32 (24 on slow
  floor surfaces).
- While speed is at or below 0: speed increases by 1.1 per frame.
- While speed is at or below target: speed increases by (1.1 − speed/43)
  per frame — a decaying acceleration that asymptotically approaches about
  47, but the target cap takes over first at full stick.
- While speed is above target and the floor is flat (normal.y ≥ 0.95): speed
  decreases by 1.0 per frame.
- Speed is hard-capped at 48.

Facing: Mario's facing angle approaches the intended stick yaw at up to
0x800 angle units per frame. Slope adds or subtracts speed scaled by slope
steepness: 5.3 (very slippery), 2.7 (slippery), 1.7 (default), 0.0
(non-slippery) per frame, applied downhill/uphill relative to facing. Quicksand
deeper than 10 units scales the target speed down by 6.25/depth.

Transition thresholds documented on the page: B with speed ≥ 29 and stick
magnitude > 48 dives (giving 20 upward speed); the stick held back with speed
≥ 16 turns Mario around; neutral stick or first-person view with speed ≥ 16
brakes, below that decelerates; jumping out of a landing chains into single,
double, or triple jump depending on the previous landing action, the double
jump timer, and (for the triple) speed > 20 with the wing cap considered.

The page includes a speed-over-time graph (first 45 frames). The published
SVG is a smoothed illustration with only 11 curve points — not per-frame
measured data — so no per-frame series was transcribed; the scalar anchors
above are in `walking_behavior.csv`.

## Quarter steps (wiki:Movement steps@19723)

Position is updated in "steps". On the ground and in the air, movement is split
into 4 quarter-steps per frame, each moving 1/4 of the frame's velocity.
Quarter steps do not check object collisions or warps. The published checks
per quarter-step (air):

- Landing: if the new position is within 78 units below a floor, Mario lands.
- Wall: if the new position is above the current floor but blocked, Mario hits
  a wall.
- Riding shell on water: the water surface counts as a floor up to 78 units
  above the position.
- Ceiling: a ceiling within 160 units above with non-negative vertical
  velocity zeroes the vertical velocity; hangable ceilings are checked in the
  same 160-unit window.
- Falling more than 1150 units below the peak plays the falling scream.
- 100 units below the water level transitions to the water plunge.

Related: "De facto speed" (wiki:De facto speed@14131) is defined as horizontal
speed times the floor normal's y component — it shrinks as slopes get steeper
and is used in place of raw horizontal speed in several checks.

## Jump, dive, long jump, gravity

Single Jump (wiki:Single Jump@19309): on entry, vertical speed becomes
42 + forward_speed/4 and forward speed is multiplied by 0.8 (halved vertical
if squished or in quicksand deeper than 1). From a single/double jump, B with
speed > 28 dives, otherwise jump-kicks.

Dive (wiki:Dive@19303): on entry, horizontal speed increases by 15 (then
clamped to the 48 cap); dives started from ground actions also grant 20
vertical speed. Requires speed ≥ 29 from walking/decelerating/crawling,
> 28 from single/double jump.

Long Jump (wiki:Long Jump@14374): initial speed above 16 selects the fast
long-jump animation; otherwise the slow one.

Gravity (wiki:Gravity@20294), per frame, first matching branch: twirling
(−4, scaled by yaw speed when spinning fast, terminal −75); shot from cannon
(−1, terminal −75); long jump/slide kick (−2, terminal −75); lava boost
(−3.2, terminal −65); metal cap underwater (−1.6, terminal −16); default
(−4, terminal −75). Additionally, when an action controls jump height and
vertical speed exceeds 20 while A is not held, vertical speed is quartered.
Wing-cap glide clamps the fall to −37.5.

All anchors above are transcribed in `traces/public-anchors/`
(`action_ids.csv`, `walking_behavior.csv`, `quarter_steps.csv`,
`airborne_numbers.csv`).
