#!/usr/bin/env bash
# testbox side of tools/build-ab.sh: runs the jobs in <jobs> (lines "<variant> <run> <env...>") one after another
# against one BDS port with the prebuilt swarm_build, appending "<variant> <run> <port> PASS|FAIL <secs> <ticks>"
# to <out>/results; each run's log and journals stay in <out>/<variant>-<run>/.
# usage: tools/build-ab-worker.sh <bin> <port> <jobs> <out>
set -uo pipefail
bin=$1 port=$2 jobs=$3 out=$4
while read -r variant run env; do
  dir="$out/$variant-$run"
  mkdir -p "$dir"
  # shellcheck disable=SC2086
  (cd "$dir" && env RUST_LOG=warn BUILD_JOURNAL="$dir/journal" ${env//,/ } timeout 1200 "$bin" "127.0.0.1:$port" BuildA BuildB BuildObs >run.log 2>&1)
  line=$(grep -E '^(PASS|FAIL) with' "$dir/run.log" | tail -n 1)
  status=$(awk '{print $1}' <<<"$line")
  secs=$(sed -nE 's/.*\(([0-9]+) s\).*/\1/p' <<<"$line")
  ticks=$(sed -nE 's/.*: ([0-9]+) ticks.*/\1/p' <<<"$line")
  echo "$variant $run $port ${status:-ERROR} ${secs:--} ${ticks:--}" >>"$out/results"
  # A killed run's players linger on the server: the next run's join would be kicked (ServerIdConflict).
  [ -n "$status" ] || sleep 30
done <"$jobs"
touch "$out/done.$port"
