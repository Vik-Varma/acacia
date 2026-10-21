# Contributing to Acacia

Thanks for helping. Bug reports, packet captures and pull requests are all welcome. For anything bigger
than a fix, open an issue or ask on [Discord](https://discord.gg/DNFJBJBVPr) first, so the design fits
[docs/DESIGN.md](docs/DESIGN.md).

## Setup

Rust 1.98 or newer. `cargo test --workspace` needs no server or account. To test against a real
server, set up a local BDS as [docs/testing.md](docs/testing.md) describes.

## Before you open a pull request

CI runs these, with warnings as errors:

```sh
cargo clippy --workspace --all-targets
cargo test --workspace
```

- Tests that need a live server or a real account are `#[ignore]`d. Run the ones near your change by
  hand and say so in the pull request.
- Physics and bot changes also run the trace regression set:
  `cargo test -p acacia-bot --test trace_regression`. A trace that gains mismatches fails it.

## Architecture rules

- Crate dependencies point downward only ([docs/DESIGN.md](docs/DESIGN.md)).
- `acacia-raknet`, `acacia-proto` and `acacia-session` stay network-free: no sockets, no async, no tokio.
- `acacia-proto` is generated. Change `tools/codegen`, never its output ([docs/proto.md](docs/proto.md)).
- Behaviour copies the vanilla client. A change to what acacia sends or how it moves needs evidence: a
  capture from a vanilla client or a recorded trace. Traces that guard a change go in
  `crates/acacia-bot/tests/traces`; a change to what a packet field carries is written in that crate's
  README.

## Style

- There is no rustfmt configuration. Don't reformat files you aren't changing; match the code around
  your change.
- Comment only what the code can't say: a non-obvious rule, an edge case, a vanilla quirk being copied.
- Update the docs your change makes wrong in the same pull request.
- One topic per pull request. Say how you tested it.

## Licensing

- Contributions are under the [MIT License](LICENSE).
- Don't copy or port code from GPL or LGPL projects (Pumpkin, PocketMine-MP, PowerNukkitX and others).
  Citing how they behave is fine.
- Code ported from an MIT-compatible project keeps its notice: add it to [THIRD-PARTY.md](THIRD-PARTY.md).
- Never commit account tokens, `.tokens/` or Mojang's game files.

## AI tools

You may use them. You are responsible for every line you submit: read it, run it and be able to explain
it. Say in the pull request if most of a change was generated.
