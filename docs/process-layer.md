# Process layer (roadmap C2)

Code: `crates/acacia-bot/src/pathfind/process/`. Decisions (user, 2026-10-17): caller-owned manager; Backfill in
trail mode, nearest block.

## Shape

```rust
let mut procs = Processes::new();
let goto = procs.add(Goto::new(goal), 10);
procs.add(Follow::player("Steve", 3.0), 50);
procs.add(Backfill::new(), 60);
bot.run(&mut procs).await?;            // or procs.tick(&mut bot) once per tick
procs.pause(goto); procs.resume(goto); procs.remove(goto);
procs.outcome(goto);                   // Some(Ok(())) once arrived
```

- `Process` trait: `want(&Ctx) -> Want` each tick, plus hooks (`opts`, `arrived`, `failed`, `acted`, `observe`,
  `lost_control`, `regained`). `Want` is `Go(Goal)`, `Hold`, `Act(Act)`, `Yield` or `Done(result)`.
- `Arbiter` (network-free) asks the processes in priority order (higher first; ties in the order added) and stops
  at the first one that does not yield. Paused processes are skipped. `Processes` wraps it for a server, and the
  sim tests drive it on a grid.
- One pilot drives everything. It is rebuilt when the controlling process or its goal changes, and it keeps the
  shared rollout cache (keyed by profile and player). If the same goal returns after a tick that didn't drive it,
  `Pilot::interrupt` drops the path (later searches favour it) and any search started before. Neither counts as a
  re-plan.
- `Bot::goto`/`goto_with` run a single `Goto`. They behave as before: they return on arrival without waiting a
  tick, and a failure becomes the outcome.
- Work is not pre-empted mid-step: a path's dig/place/door work and an `Act` run to completion inside one
  `tick`, so control can only change between ticks.
- `Ctx::quiet` counts ticks since another process last had control, so a low-key process can wait for an idle
  bot. `Event::Dug` reaches every process: `goto::step` records the blocks a path's work broke, with their
  identifiers, whether the work failed or not.

## Processes

- `Goto`: `Go(goal)`. It finishes on arrival and fails when the pilot gives up (replans, give-up guard).
  Pre-empted, it re-plans from wherever it gets control back.
- `Follow` (needs `BotConfig.trackers.entities`): while the target is tracked it holds control. It drives to
  `Near(target cell, range - 1)` and re-targets once the target has moved 2 blocks. Within `range` it holds,
  with 1.5 blocks of hysteresis. It yields while the target is untracked, and for 40 ticks after the pilot gives
  up on it.
- `Backfill`: collects `Dug` cells. A cell is open once it is still air, not on the path ahead (a node's feet
  or head cell) and not in the player box. It fills an open cell within 4 blocks of the eye, on the ground, as
  an `Act::Fill`. Material is the original block, then `substitutes()` (stone → cobblestone, grass → dirt, ores →
  cobblestone), then the scaffold list. On a server the fill counts only when the server shows the block
  (`place_block` waits for it). After 3 failures, 10 ticks apart, it gives a cell up. After 20 quiet ticks it
  walks back to open cells within 16 blocks. Cells further than 32 blocks are given up. Give it a higher
  priority than the processes whose digs it fills, so it can take over between their moves.

## Evidence

- Sim (`pathfind/tests/process_sim.rs`): Follow pre-empts Goto, then Goto resumes and arrives when the target
  leaves and when Follow is paused. Backfill restores all 12 cells of a 6-deep tunnel, with cobblestone for stone.
- Live, strict BDS (`examples/processes.rs`, test pack off, fresh world, 2026-10-17):
  - backfill: 8 dig moves, every cell filled, 0 corrections.
  - follow: Follow in control for 253 of 311 ticks, closest 2.03 blocks, then the goto arrived, 0 corrections.

## Open

- A cell the bot stands in at the end stays open until it leaves.
- Backfill's walk-back can pull the bot away from a process that holds (Follow standing) after 20 quiet ticks
  only if Backfill outranks it; it doesn't know what lower processes want.
- A `Follow` goal is a fixed `Near` that re-targets in 2-block steps. It is not a moving-target goal that
  predicts the target's motion.
