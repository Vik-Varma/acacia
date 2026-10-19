# Schematic builder (B1, B2)

`acacia_bot::build`: a `.mcstructure` placed block-exact by a [`Builder`] process (C2), every break and place
server-confirmed (C3). Roadmap: `baritone-roadmap.md` B1.

```rust
let mut job = BuildJob::new("house.mcstructure", Transform { origin, turns: 1, mirror: false });
job.stations = vec![barrel_a, barrel_b];
std::fs::write("job.json", job.to_json())?;          // all a restart needs
let builder = Builder::new(job)?;
let status = builder.status();                        // Arc<Mutex<BuildStatus>>, readable while it runs
let mut procs = Processes::new();
procs.add(builder, 10);
bot.run(&mut procs).await?;
```

## Decisions (user, 2026-10-17)

- Mismatched cells are cleared: anything differing from the structure, air cells included, is broken and
  replaced. `structure_void` is left alone. `BuildJob::fill_only` places into empty cells only (others: `kept`).
- Resume = re-survey. No progress log: only the job (structure path, transform, stations, options) is
  serialisable. Every tick the builder diffs the world against the structure, so a fresh builder after a restart
  carries on from what is there. Unloaded cells are `unknown` and visited.
- Block set: full blocks, logs/pillars, slabs (both halves, double), stairs (facing, upside-down), facing blocks
  (furnace, observer, piston, dispenser), fences/walls/panes, glass, torches, ladders, buttons. Everything else is
  reported `unsupported` and skipped (doors, beds, signs, redstone, fluids, crops, chests...; `rules::DENIED`).

## Placement rules (`build/rules.rs`)

Measured on strict BDS 1.26.52.3 with `examples/place_probe.rs` (every item from four sides, every visible face,
clicked low and high; `VERT=1` for six-way blocks close above and below). Tests: `build/tests/rules.rs`.

| State | Decided by |
|---|---|
| `pillar_axis` | clicked face's axis |
| `vertical_half`, `upside_down_bit` | Down face or a side click above 0.5: top; else bottom |
| `weirdo_direction` (stairs) | player's horizontal look: east 0, west 1, south 2, north 3 |
| `cardinal_direction` (furnace) | towards the player |
| `torch_facing_direction` | Up: `top`; side: opposite of the clicked face; Down is not a hang (BDS picks a side) |
| `facing_direction` ladder, buttons | clicked face (ladders refuse Up/Down) |
| observer | look direction, vertical too (from close above: `down`) |
| piston | look direction horizontally, towards the player vertically |
| dispenser | towards the player, vertical too |
| six-way vertical | eye over 2 above the block's bottom, or below it, and within a block sideways (measured vertical at 1.0, horizontal at 1.9; between is avoided) |
| corner, connections, wall posts, powered/triggered/pressed bits | derived by the server: never compared |

A second slab clicked into a cell holding a half slab makes the double slab (whatever face). A double slab's
`vertical_half` is not compared. Clicks land only on a block's shape: full cubes anywhere, a half slab on its own
half of the side faces; panes, fences, walls, torches and other partial blocks are never clicked against
(`reach::points`). Interactive blocks are not clicked against either.

## Each tick (`build/plan.rs`)

1. Survey: each cell is Done, Place, Break, Drain (liquid where air belongs), Kept, Unsupported or Unknown.
2. Breaks and drains, top down. A liquid next to the cell (above or a side) that is not a solid target is sealed
   first with a scaffold block; seals outside the footprint stay (`status.seals`). Scaffold the pilot stands on in
   a cell where air belongs is left until the cleanup.
3. Places, bottom up, nearest first: first anything clickable from where the bot stands, then a walk to a stand
   for the lowest. A cell is never placed while the cell under it is still open (covering it takes away the
   click on the face below). A placement's state depends on the click and the player's facing, so a stand is a
   spot whose eye makes some click give the target (`reach::stands`, `reach::click_for`), with extra reach slack
   for the pilot stopping off-centre. Stands are keyed as the pathfinder keys standing spots: on a bottom slab
   the spot is the slab's own cell, feet 0.5 up (a cell above a bottom slab is no spot, and goals there were
   unreachable). Stands may be over air: the pilot pillars up (`allow_bridge`), never through cells the
   structure still has to fill, and the blocks it places arrive as `Event::Placed` and count as scaffold. A
   stand's feet and head stay out of cells the structure still fills (live: the bot stood in the unbuilt top of a
   ladder column and reached nothing from there). Stands are driven to with a 0.1 arrival tolerance: at the
   default 0.25 the body overlaps the cell next to the stand and the click is never allowed.
   Gravity blocks wait for their support; a cell with nothing clickable around it gets a scaffold column up to it.
4. Restock when the next placement's item is missing (`build/restock.rs`): everything the open placements still
   need minus the inventory, from the nearest station not yet found without it. A station that brought no more of
   an item is marked lacking it (cleared after 30 s of waiting).
5. Unknown cells: walk near. Then cleanup: scaffold where air belongs or outside the footprint, top down.
6. Done once every cell is right and no scaffold stands; the outcome is an error if cells were given up.

Failures: an act failing 4 times, a cell placed wrong more than 3 times, or 45 rests with nowhere to do it from
gives the cell up (`status.failed`, with the reason). A cell given up only for want of a stand is tried again
(up to 3 times) once nothing else can be done: the build around it changes what reaches it. A cell with no stand
rests 40 ticks so others go first. A drive the pilot gives up on (no route, a refused pillar) is not the cell's
failure: the stands it drove to are left out for 400 ticks and the task rests. The committed task
comes first in every rank and keeps its goal while open: re-picking the nearest each tick switched goals as the
bot moved, and every switch restarts the pilot (live: always moving, never arriving). A goal just reached that
still allows no click rests its task instead of being asked for again (the arbiter would hold still). In the air
the builder keeps driving the committed goal, so jumps and pillars finish.

`Builder::explain_for(bot)` lists each open placement with its held count, the click it has from where the bot
stands and the stands it could use: the first thing to read when a build stalls (`examples/build.rs` prints it).

## Evidence

- Sim (`pathfind/tests/build_tests.rs`, own BDS placement model in `build_sim.rs`): mixed 5×3×5 house with
  mismatches, restock by need, resume, support column cleanup, liquid seal; house16 (`--ignored`, release)
  block-exact in 2231 ticks.
- Live, strict BDS 1.26.52.3, fresh world, test pack off (`examples/build.rs`, 2026-10-17): house16 from two
  barrels, junk in the footprint, the bot disconnected after 150 placements and a fresh one resumed from the
  saved job. Observer diff 0 cells; 588 placed, 29 broken, 4 restocks, 32 corrections, 2357 ticks (512 s).
- An offline bot that reconnects to BDS comes back with an empty inventory (position kept): what the killed bot
  carried is lost, so the live stations hold the house twice over.
- Swarm sim (`pathfind/tests/swarm_sim.rs`, bots ticked in turn on one grid, each with its own inventory):
  house16 by two bots in 976 ticks against one bot's 2231 (0.44×), block-exact, no scaffold left; a bot stopped
  at tick 80 has its tile taken over after the lease and the other finishes; two bots restocking a 16×16 floor
  from a barrel holding exactly the floor both build, neither holding more than two stacks. Board rules:
  `build/tests/board.rs`.
- The B1 pass above needed its restart: without one, house16 failed on the last junk cell broken before the
  placing ([2, 0, 2], "placed wrong"), also at B1's commit. BDS kept destroying that cell (planks gone every
  3.3 s, their break time without an axe) because breaks never ended; every break now ends with `AbortBreak`
  (`interact::wire::break_step`; `StopBreak` does not end it: junk scene, Stop failed, Abort passed).
- Live scenes (`examples/build.rs` `SCENE=`, seconds each, regression checks; fresh world, stock 20 TPS):
  `wall` (wall with log ends and slab rim) PASS 46 s; `junk` (floor under a furnace where junk stood) PASS 43 s
  with Abort; `ladder` (wall over a floor with a ladder column) PASS 75 s (843 ticks before the 0.1 stand
  tolerance, 2079 with the stand rule alone; 230 after), also at 2× TPS (`ACACIA_TICK_SCALE=2`).
- Live house16 one bot, no restart (`examples/swarm_build.rs BOTS=1`): PASS, 454 s, observer diff 0.
- Live house16 two bots (tile 4): PASS each time, 266/298/296/275 s against one bot's 454 s (0.59-0.66, mean
  0.63): the 0.6 bar is not met. Kill run (B dropped after 150 placements, never back): PASS 394 s, diff 0.
- Where a placement's time goes (`act_timing`, printed by the examples; one bot, house16, 470 s): acts 323 s,
  of them equip with a hotbar switch 198 s (235 switches, mean 842 ms: the human equip pause), echo wait 79 s
  (mean 120 ms), the tick after each act 31 s, aim 28 s, mining 24 s; planning CPU 3 s.
- Switches without the container-click pause (Human: a 50-200 ms key press; Optimal: none), held item ranked
  first, no clicks onto interactive blocks: one bot 271 s (-42%), switch time 2.9 s (191 switches), retries 3
  (were 42: a pillar started on a station barrel, whose click opens it). Ladder scene 55 s (was 74).

## Swarm (B2: `build/board.rs`, `build/swarm.rs`)

Several builders share one job through a `TaskBoard` (`Builder::with_board(board, name)`; without one a builder
is exactly B1). `LocalBoard` serves bots in one process; a networked board can implement the same trait.

- Regions: square tiles of the footprint (`BuildJob::tile`, default 4; cells outside the footprint, such as
  scaffold, belong to the nearest tile). A bot claims the nearest tile with open work (a cell to place, break,
  drain or visit, or scaffold to remove) that no other bot holds, and acts only inside it. With 8 (four tiles
  on house16) one bot idled through the tail while the other finished its tile: 361 s against 266-298 s with 4;
  2 was worse again (295-309 s). Tiles are a stopgap: see "Open".
- Leases: a builder joins the board when given it and heartbeats every 20 ticks with its feet; a lease lapses
  after the board's lease time without one (live: 10 s; vanilla-tick time under `ACACIA_TICK_SCALE`), and the
  tile goes back to the pool. The board stores no progress: whoever claims a tile next surveys it, so a dropped
  bot's half-done tile needs nothing special.
- Support across borders: placements still need a solid neighbour in the world, so a cell whose support is in an
  unbuilt neighbouring tile simply isn't doable yet. A tile with nothing doable is given back and skipped for 60
  ticks, and the bot claims another; a bot finishes only when the whole job is done and no scaffold stands.
- Stations: a bot restocks its share of the job's remaining need (the need over the bots on the board: two bots
  each taking the whole need would empty the barrel, and per small tile they walked back after nearly every
  tile), and one bot at a time opens a station (`open_station`, held under the same lease).
- Room: stands within 1.5 blocks of another bot are not used, no block goes into another bot's body, and pillars
  stay out of tiles other bots hold. Scaffold is shared through the board, so whoever holds its tile removes it.
  The pilot itself does not see other bots: its terrain memo, rollout cache and sim-verified macro moves are
  keyed on blocks only, so moving obstacles there is not a small change.

## Open (2026-10-19)

The two-bot ratio. Next, in order: group placements by item and skip the equip pause under the Optimal
profile; pipeline placements (predict, send without awaiting each echo, governor in-flight cap); aim and click in
one tick without the post-act tick; stands chosen for several cells; then a dependency-graph ready-set scheduler
with soft regions instead of tiles (`research/build-scheduling-robotics.md`, `research/build-scheduling-game-bots.md`).

## Not in B1/B2

- Portable shulker restock (place, take, break, pick up); stations must be placed containers.
- Crafting missing items; dumping junk when the inventory is full (drops from clearing are picked up).
- Line of sight: clicks only need the face to point towards the eye.
- Other bots as pilot obstacles; a networked board.
