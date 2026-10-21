# netem

A UDP proxy that makes a link worse: delay with jitter, random loss and a bandwidth cap, each per
direction. It speaks no protocol, so it works for RakNet, NetherNet's UDP and a real client alike.

```
cargo run --release -p acacia-netem -- --listen 127.0.0.1:19196 --to 127.0.0.1:19132 \
    --delay 75ms --jitter 10ms --loss 2 --down-rate 2mbit --down-queue 256k
```

- `up` is client to server, `down` server to client. `--delay`, `--jitter`, `--loss` (percent),
  `--rate` (bits a second: `kbit`, `mbit`) and `--queue` (bytes waiting for the rate: `k`, `m`) set
  both directions; `--up-<option>` / `--down-<option>` set one.
- Each client gets its own socket to the server, so the server sees one peer per client.
- A datagram is lost first, then waits its turn on the capped link (dropped when the queue is full),
  then is delayed; arrivals never overtake each other.
- `--stats N` prints a line every N seconds: per direction the datagrams in, lost, dropped for a full
  queue, sent, kB sent and the largest queue seen. `--for N` stops after N seconds.

## Timing sapling joins

`tools/netem-pacing.sh <local sapling checkout> [radius]` runs `pacing.sh` on testbox:
- a sapling `drill_server` on generated terrain;
- one warm-up join straight to it, so later joins time the link and not generation;
- one `join_timing` bot through netem per profile (clean, lossy, narrow, bad).

Each prints the time to spawn, to every column of the view and to the last section answer.
