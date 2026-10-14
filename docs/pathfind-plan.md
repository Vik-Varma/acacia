# Pathfind: Baritone-style planner

Branch `pathfind/parkour`. Prior art (ideas only, MIT, no code copied): `../research/mc-zuri-pathfinder.md`.
Paths below are under `crates/acacia-bot/src/` unless marked.

## Decisions (user, 2026-10-13)

- Extend the existing A* (`pathfind/astar.rs`), keep Baritone tick costs and existing moves/tests.
  Add mode-tagged nodes and **macro moves** verified by forking `acacia-physics` `PlayerState`.
  Planner and follower share one controller per move; follower re-verifies every tick and
  replans in place on divergence.
- Milestones: 1 ground speed & parkour → 2 elytra + fireworks → 3 boats → 4 entity avoidance.
- `MovementProfile {Optimal, Human}` drives planner and follower. Human applies everywhere
  (walk steering too): eased/jittered aim, plans only moves that land under timing/aim
  perturbations with an edge margin.
- Heuristic stays admissible: per-profile ticks-per-block measured from steady hop speed when
  macros are on, 3.563 otherwise.
- Flat-ground hop chains on by default; dropped when food ≤ 6. Gap leaps unaffected.
- Human aim constants are placeholders in `human.rs` until a vanilla capture fits them.
- Neo/ladder jumps: stretch at the end of M1, slip to M2 if they stall.
- 2026-10-14, after `../research/pathfinder-survey.md` (Tungsten/unionclef, azalea, ParkourCalculator,
  SwarmBot): nodes are momentum-aware; follower gets an any-angle shortcut; leaps search takeoff
  yaw/point and in-air correction with an obstruction prefilter.
- Done bar for M1: headless sim tests per move under both profiles; parkour legs in
  `examples/course.rs` with 0 corrections on a dedicated strict testbox BDS; ticks-to-goal
  comparison vs macros off; `trace_regression` green.

## M1 design

### Node state (`pathfind/node.rs`)
- `Mode: u8` (`Ground` now; `Glide`, `Boat` later). `Entry: Rest | Run(dir8, speed_bin) | Air` (Air: start only).
  `speed_bin` (horizontal speed, ~4 bins) keeps a faster later arrival from being pruned by a slower
  earlier one; if bins prove too coarse, keep a small non-dominated set per key instead.
- `NodeKey {pos, mode, entry}` replaces `BlockPos` as the A* key (`astar.rs` `Rec`, `PosMap`).
- `Momentum {off:[i16;3] (1/1024 blk), vel:[i16;3] (1/4096 blk/t), yaw_q, flags}`;
  `snap(&PlayerState)` / `synth(pos, Momentum) -> PlayerState`. Stored per `Rec`.
- Classic and alter edges yield `Rest`; macro edges need `made == 0` at both ends.
- `PathNode` gains `entry` and `mac: Option<Macro>`; `MoveKind` gains `Run`, `Hop`; gap leaps stay `Parkour`.
- As built (steps 1–2): `Momentum::snap(st, at)` / `synth(at, base)`; `base` (live state) supplies
  attribute, freeze, effects, equipment, flight; flags = on_ground, sprinting, sneaking, pressing_jump,
  jump_delay. Fidelity held (worst 0.0013 blk over 45 snaps × 30 ticks), no full-`PlayerState` fallback.
  `Run(dir, bin)` bins via `node::speed_bin` (edges placeholder until step 6). `Momentum` per `Rec` lands
  with the first macro edge (step 4).

### Shared sim
- `movement/keys.rs`: `KeyLatch` lifted from `Movement::tick` (double-tap, sprint latch, wire
  rotation) — the only Controls→Input mapping. `Movement::fork() -> Rollout`.
- `world/adapter.rs`: extract `block_physics`; `pathfind/physview.rs`: `WorldView for Terrain<B>`
  (unloaded = `Blocks::ids` → None). `Cell` gains `liquid`.
- `pathfind/rollout.rs`: `Rollout {st, keys}`, `step`, `run(ctl, max) -> Trace {bodies, landed, ticks, min_margin}`.
- As built: `Rollout {st, keys, using_item}` + `step` live in `movement/fork.rs` (keeps pathfind →
  movement one-way); `rollout.rs` has `Body`, `run(r, ctl, c, max, w)`, `peek`. `min_margin` comes with
  leap acceptance (step 4).

### Controllers (`pathfind/control.rs`, `pathfind/profile.rs`)
- `trait Controller { fn step(&mut self, b:&Body, peek:&mut dyn FnMut(&Controls)->Body, c:&mut Controls) -> Phase }`.
- `RunCtl {dir, cells}`; `LeapCtl {target, jump: AtEdge|AtTick(k)|AtAlong(x), coast, aim}`; `HopCtl`.
- `Aim`: Optimal snaps; Human eases with capped rate + splitmix jitter.
  `profile.perturbations()`: Optimal `[nominal]`; Human nominal, jump ±1 tick, yaw ±jitter, start ±0.05.

### Macro moves (`pathfind/leap.rs`, `pathfind/hop.rs`)
- Leap from `Stand && !wet`: cardinal 2–5, diagonals (2,1)(2,2)(3,1)(3,2)(4,1); dy +1, 0, −1..−3
  (deeper only into water). Geometric prefilter (corridor `body_clear`, ≥1 gap cell) then
  jump-timing search (AtEdge/AtAlong, AtTick 0..6) × coast {None,3,5}; fewest ticks wins.
- Before timing search: SwarmBot-style obstruction prefilter (drop targets whose straight corridor
  is blocked). Timing search also tries takeoff yaws (±11.25°, ±22.5° off the target line), a free
  takeoff point along the last block (ParkourCalculator-style), and an in-air yaw correction.
- `Run(d, 1..3)` only before a gap or on lanes ≥4; `Hop` only from `Run(d)` along d.
- Accept: on_ground, feet within 1e-3 of spot, xz in target cell, no AVOID/danger touch, no
  pre-apex h-collision, ≤40 ticks. Human: all perturbations land in the cell, edge margin ≥0.15.
- Cost: simulated ticks (worst perturbation for Human) + `DANGER_NEAR`; Human + `HUMAN_JUMP` per
  jump + `RISK*(1-margin/0.3)`. Classic `gap_jump` only when `!macros`.
- As built (step 4): `macros.rs` (successors), `solve.rs` (timing search, shared with the follower), `leap.rs`
  (`LeapCtl`). Cardinal reach 7, not 5: from rest a 4-gap needs a 2-block run-up inside the leap (a jump from the
  edge block falls short). Prefilter = walk the lane, a blocked cell shadows the rest. Timings: `Tick(0..=12)`
  deduped by actual takeoff tick, coasts 8..0 only on overshoot; candidates judged fastest-first. Human = every
  perturbation lands, hazard pad 0.25, no absolute 0.15 margin (survey rec 5); Human refuses 4-gaps. Search starts
  from the live state (`search_from`); test `opts()` keeps macros off so the classic suite still runs.
- As built (step 5): targets in `leap_moves.rs` (all 8 symmetries of the diagonals; dry dy +1..−3, first water
  below up to 16). Solve = earliest takeoff that lands wins (air time barely varies), coast by binary search;
  `Never` (run off the edge) for drops — a jumped down-2 falls 3.25 and is rejected (fall > 3). Takeoff yaws
  ±11.25/±22.5 + air aim only when a diagonal misses straight. Head-hitters: any ceiling contact rejects.
  Hazard pad is sideways and up, not below the feet (lava level with the floor is cleared).

### Cache and budget (`pathfind/cache.rs`)
- Key `(family, params, dir, Momentum, corridor signature hash)`; caches failures too.
- `SearchOpts` gains `profile`, `macros`, `hops`, `max_rollout_ticks` (400k); spent → classic only.

### Follower (`pathfind/follow_macro.rs`)
- `Follower::tick(&mut self, s, live: Option<Rollout>, w: &dyn WorldView, c)`; hands off to
  `Exec {ctl, predicted, t, target}` for macro nodes.
- Per tick compare live `Body` to `predicted[t]` (Optimal 1e-4, Human 0.03); on mismatch
  re-predict from live, retry other timings, else `FollowStatus::Diverged`.
- Navigator on `Diverged`: no stop, not counted in `max_replans` (cap 5 consecutive); hold
  controls while airborne; inline quick search from live `Momentum` (Run entry).
- Invariant test: Optimal executed ticks == predicted.
- As built (step 3): `tick(s, live: Option<&Rollout>, w, c)`; `Exec` sits in `MacroRunner`; "retry other
  timings" waits for the step-4 timing search (TODO in `follow_macro.rs`); the in-place re-plan searches
  from the live position (Rest) until Run entries exist (TODO in `goto.rs`). `LeapCtl` ignores air before
  its first ground contact. A full sprint jump overshoots a cell-exact landing, so coast/timing search
  is required, not optional (step-3 test picks the coast by rollout).
- As built (step 4): a leap that does not finish from the live state is solved again (`resolve`: fresh timing
  search before takeoff, coasts once airborne), at start and on drift; `MacroReport.resolved` marks it. A
  finished macro hands over to the next on the same tick. The classic follower sprints into a macro heading
  the same way (run-up). Navigator always plans from the live state, so a re-plan keeps the momentum.

## M1 steps (one commit each, tests with each)

0. Refactor only: `KeyLatch`, `block_physics`, `Movement::fork`, `Cell.liquid`. Gate: `trace_regression` + pathfind tests unchanged.
1. `physview` + `rollout` + `Momentum`. Tests: Movement vs Rollout bit-exact 200 ticks; snap/synth within 1/512 over 30 ticks.
2. `NodeKey`/`Entry` in A* (Rest only); existing tests green.
3. Controller trait, `profile.rs`, `follow_macro.rs`, `Diverged` in navigator. Tests: hand-built leap path predicted == executed; forced perturbation → replan without stopping.
4. Flat cardinal Leap 1–4 + timing search; Human detours around pixel-perfect gaps.
5. Height-changing and diagonal leaps; head-hitter rejected; lava gap margin.
6. `Run`/`Hop` + `Run(d)` entries; stepping stones chain with no Rest between; measure hop speed, switch heuristic; hunger gate.
6b. Any-angle follower shortcut (azalea-style): on Ground segments, simulate sprint / sprint-jump
    straight at nodes up to ~20 ahead, take the fastest clean rollout that reaches the path; removes
    8-direction staircasing. Test: 30-block diagonal ticks drop vs 6; Optimal predicted == executed.
7. Cache + rollout budget; perf guard (flat 100×100, 80 blocks, ≤2× macros off).
8. Human complete: aim on all segments incl. walk, perturbations, risk cost; 5 seeds, 0 Diverged failures.
9. `tests/speed.rs`: ticks-to-goal macros on vs off (flat, diagonal, gaps, stairs); on ≤ off.
10. `examples/course.rs` parkour legs (gap3, gap4, up1, down2, diag, stones, sprint40), `--profile`, `--macros off`; dedicated strict testbox BDS, 0 corrections both profiles; evidence in `docs/DESIGN.md`.
11. Stretch: neo jumps (Optimal), ladder jumps.

## Running
- Tests: `REMOTE_DIR=work/bc-pathfind tools/remote-test.sh -p acacia-bot` (testbox; never workspace-wide locally).
- Risks: synth fidelity (fallback: store full `PlayerState` per macro node); in-air yaw swings on strict BDS; node/rollout blowup on open maps.
