#!/usr/bin/env bash
# Times a bot's join to a sapling drill server over links made worse by tools/netem, on testbox:
# syncs the sapling checkout at $1 there, then runs tools/netem/pacing.sh with remote-run.sh.
# usage: tools/netem-pacing.sh <local sapling checkout> [radius]
# REMOTE_DIR is this tree's dir on testbox (remote-run.sh), SAPLING_REMOTE sapling's (work/sapling-pacing).
set -uo pipefail
sapling=${1:?usage: tools/netem-pacing.sh <local sapling checkout> [radius]}
remote_sapling=${SAPLING_REMOTE:-work/sapling-pacing}
opts="-o ConnectTimeout=20 -o ServerAliveInterval=10"
tar czf - -C "$sapling" --exclude=target --exclude=.git --exclude=./research --exclude=./docs . \
  | ssh $opts testbox "mkdir -p $remote_sapling && find $remote_sapling/crates -name '*.rs' -delete 2>/dev/null; tar xzf - -C $remote_sapling" || exit 1
TIMEOUT=${TIMEOUT:-3600} STALL=${STALL:-1800} TAIL=${TAIL:-20} "$(dirname "$0")/remote-run.sh" env SAPLING_DIR="$remote_sapling" RADIUS="${2:-8}" PORT="${PORT:-19240}" bash tools/netem/pacing.sh
