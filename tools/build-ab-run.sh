#!/usr/bin/env bash
# testbox side of tools/build-ab.sh (runs as its detached job, so a local reap can't stop it): builds swarm_build,
# deals the runs over the ports (variants interleaved), runs one worker per port, then prints the per-variant
# summary. Results: build-ab/results; each run's log and journals: build-ab/<variant>-<run>/.
# usage: tools/build-ab-run.sh <runs> <variant>...   (PORTS="19184 19194 19196")
set -uo pipefail
runs=$1
shift
read -ra ports <<<"${PORTS:-19184 19194 19196}"
cargo +stable build --release -p acacia-bot --example swarm_build 2>&1 | tail -n 3
[ "${PIPESTATUS[0]}" -eq 0 ] || exit 1
out=$PWD/build-ab
rm -rf "$out"
mkdir -p "$out"
k=0
for ((r = 1; r <= runs; r++)); do
  for v in "$@"; do
    case $v in *:*) e=${v#*:} ;; *) e= ;; esac
    echo "${v%%:*} $r $e" >>"$out/jobs.${ports[$(( k % ${#ports[@]} ))]}"
    k=$(( k + 1 ))
  done
done
for p in "${ports[@]}"; do
  [ -f "$out/jobs.$p" ] && bash tools/build-ab-worker.sh "$PWD/target/release/examples/swarm_build" "$p" "$out/jobs.$p" "$out" &
done
wait
echo "== results"
cat "$out/results"
echo "== summary"
awk '{ n[$1]++; if ($4 == "PASS") p[$1]++; if ($5 != "-") { s[$1] += $5; q[$1] += $5 * $5; c[$1]++ } }
  END { for (v in n) { m = c[v] ? s[v] / c[v] : 0; sd = c[v] > 1 ? sqrt((q[v] - c[v] * m * m) / (c[v] - 1)) : 0;
    printf "%-12s %d/%d pass  mean %.1f s  sd %.1f s\n", v, p[v], n[v], m, sd } }' "$out/results"
