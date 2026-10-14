# Roadmap: a better Baritone in acacia

Sources: `../research/baritone-gap.md`, `../research/anarchy-automation.md`, `../research/pathfinder-survey.md`.
Movement milestone detail: `pathfind-plan.md`.

## Decisions (user, 2026-10-14)

- Judge on strict BDS first; add a Geyser/Java-anticheat compatibility profile later (`tools/lab.ps1`).
- Flagship after the shared core: **building + logistics**, starting with a general `.mcstructure`
  schematic builder (litematic later); map art and highways are presets on top.
- Single bot first, swarm-ready: jobs are claimable regions/tasks with leases, so more bots is config.
- Control surface: typed async Rust library API only. Chat/CLI/viewer front-ends are later wrappers.

## Order

| # | Milestone | Done bar |
|---|---|---|
| M1 | Ground speed & parkour (`pathfind-plan.md` steps 7–11) | plan's M1 done bar |
| C1 | Segment planning ahead + splice, previous-path favouring, failure timeout | 500-block goto over 3+ segments, no stop at joins |
| C2 | Process layer (priority, pre-empt, pause/resume) with Goto, Follow, Backfill | Follow pre-empts and resumes Goto; Backfill restores every dug cell |
| C3 | Server-confirmed actions: place/break as tasks done only on the server's echo, timeout + retry, pause on correction; one lag/rate governor every module asks | dig/bridge courses with injected lag: 0 ghost blocks, 0 loops |
| B1 | Schematic builder: `.mcstructure` load, layer/support ordering, scaffold + cleanup, liquid sealing, registered stations, restock by remaining need (shulker/ender chest), resumable job | 16×16×8 mixed-block build block-exact on strict BDS, survives a restart mid-build |
| B2 | Swarm build: task board, claims, expiring leases, dropped bot's work returns to pool | 2 bots ≤ 0.6× 1-bot time; killing one mid-build still completes |
| B3 | Presets: map art (staircased, colour-need restock), highway (dig + pave + repair) | 128×128 map art block-exact; 200-block highway segment |
| R1 | Persistent swarm chunk cache; path through cached chunks; block scan + ore index | 2,000-block goto into unloaded terrain; cache survives restart |
| R2 | Seed-aware routing via sapling-worldgen's exact BDS port | goto through never-loaded chunks matches real terrain |
| R3 | Mine (veins, drop pickup, Human branch mining) | 32 iron, 0 corrections |
| M2 | Elytra autopilot overworld + nether, firework/elytra resupply | 1,000-block flight lands within 3 blocks, 0 corrections |
| M4 | Safety: mob/spawner avoidance, water-bucket fall, break blacklist | 20 runs through zombies, 0 mob damage |
| M3 | Boats; farm | 500-block boat route; 9×9 wheat over 3 cycles |
| G1 | Geyser/Java movement profile | lab course: 0 setbacks |
| V1 | Viewer path/build overlay | live path updates through a replan |

Strengths to lean on: Bedrock-exact physics (moves proven by simulation), swarm in one process,
Human/Optimal profile, headless async, a test course per capability.
