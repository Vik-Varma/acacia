#!/usr/bin/env bash
# On testbox, from this tree (tools/netem-pacing.sh starts it): a sapling drill server on generated
# terrain, one warm-up join straight to it so its columns are generated, then a join through netem
# at each profile. Prints a join_timing line and netem's last stats line per profile.
# env: SAPLING_DIR (a sapling checkout, relative to $HOME or absolute), RADIUS (8), PORT (19240; netem
# listens on the next).
set -u
export PATH=$HOME/.cargo/bin:$PATH
sapling=${SAPLING_DIR:?a sapling checkout}
case $sapling in /*) ;; *) sapling=$HOME/$sapling ;; esac
radius=${RADIUS:-8}
port=${PORT:-19240}
proxy_port=$((port + 1))

cargo +stable build -q --release -p acacia-netem || exit 1
cargo +stable build -q --release -p acacia-bot --example join_timing || exit 1
(cd "$sapling" && cargo +stable build -q --release -p sapling --example drill_server) || exit 1

SAPLING_DRILL_WORLD=generated "$sapling/target/release/examples/drill_server" "$port" >server.log 2>&1 &
server=$!
proxy=
trap 'kill $server $proxy 2>/dev/null' EXIT
for _ in $(seq 60); do grep -q started server.log && break; sleep 1; done
grep -q started server.log || { echo "the server did not start"; tail -n 20 server.log; exit 1; }

timing=target/release/examples/join_timing
echo "warm-up: $($timing 127.0.0.1:$port Warm "$radius" 2000 300)"
n=0
run() {
  local name=$1
  shift
  n=$((n + 1))
  target/release/netem --listen 127.0.0.1:$proxy_port --to 127.0.0.1:$port --stats 1 "$@" >netem.log 2>&1 &
  proxy=$!
  sleep 0.5
  echo "$name: $($timing 127.0.0.1:$proxy_port Timer$n "$radius" 3000 300)"
  echo "  $(tail -n 1 netem.log)"
  kill $proxy 2>/dev/null
  wait $proxy 2>/dev/null
  proxy=
  # The last player's columns go before the next joins.
  sleep 2
}
run clean
run "75ms each way, 10ms jitter, 2% loss" --delay 75ms --jitter 10ms --loss 2
run "25ms each way, 2 Mbit/s down" --delay 25ms --down-rate 2mbit --down-queue 256k
run "100ms each way, 3% loss, 1 Mbit/s down" --delay 100ms --jitter 20ms --loss 3 --down-rate 1mbit --down-queue 128k
