# Schematic builder (B1)

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
   structure still has to fill, and the blocks it places arrive as `Event::Placed` and count as scaffold.
   Gravity blocks wait for their support; a cell with nothing clickable around it gets a scaffold column up to it.
4. Restock when the next placement's item is missing (`build/restock.rs`): everything the open placements still
   need minus the inventory, from the nearest station not yet found without it. A station that brought no more of
   an item is marked lacking it (cleared after 30 s of waiting).
5. Unknown cells: walk near. Then cleanup: scaffold where air belongs or outside the footprint, top down.
6. Done once every cell is right and no scaffold stands; the outcome is an error if cells were given up.

Failures: an act failing 4 times, a cell placed wrong more than 3 times, or 45 rests with nowhere to do it from
gives the cell up (`status.failed`). A cell with no stand rests 40 ticks so others go first. The committed task
comes first in every rank and keeps its goal while open: re-picking the nearest each tick switched goals as the
bot moved, and every switch restarts the pilot (live: always moving, never arriving). A goal just reached that
still allows no click rests its task instead of being asked for again (the arbiter would hold still). In the air
the builder keeps driving the committed goal, so jumps and pillars finish.

`Builder::explain_for(bot)` lists each open placement with its held count, the click it has from where the bot
stands and the stands it could use: the first thing to read when a build stalls (`examples/build.rs` prints it).

## Evidence

- Sim (`pathfind/tests/build_tests.rs`, own BDS placement model in `build_sim.rs`): mixed 5×3×5 house with
  mismatches, restock by need, resume, support column cleanup, liquid seal; house16 (`--ignored`, release)
  block-exact in 1953 ticks.
- Live, strict BDS 1.26.52.3, fresh world, test pack off (`examples/build.rs`, 2026-10-17): house16 from two
  barrels, junk in the footprint, the bot disconnected after 150 placements and a fresh one resumed from the
  saved job. Observer diff 0 cells; 588 placed, 29 broken, 4 restocks, 32 corrections, 2357 ticks (512 s).
- An offline bot that reconnects to BDS comes back with an empty inventory (position kept): what the killed bot
  carried is lost, so the live stations hold the house twice over.

## Not in B1

- Portable shulker restock (place, take, break, pick up); stations must be placed containers.
- Crafting missing items; dumping junk when the inventory is full (drops from clearing are picked up).
- Line of sight: clicks only need the face to point towards the eye.
- Swarm claims: the job is per cell and stateless, so B2's leases can split it.
