#!/usr/bin/env bash
# Prints one line per testbox job that finishes from now on: "<dir> exit <code>" (124 = timed out,
# 125 = stalled; see testbox-guard.sh). Meant as a Monitor/background event source; runs until killed.
# usage: tools/testbox-watch.sh [glob]   default glob: work/bc-*
glob=${1:-work/bc-*}
list() { ssh -o ConnectTimeout=20 testbox "ls -l --time-style=+%s $glob/run.done $glob/test.done 2>/dev/null" | awk '{print $6, $7}'; }
seen=$(list)
while true; do
  now=$(list) || { sleep 30; continue; }
  comm -13 <(sort <<<"$seen") <(sort <<<"$now") | while read -r _ f; do
    [ -n "$f" ] || continue
    echo "$(dirname "$f") exit $(ssh -o ConnectTimeout=20 testbox "cat $f" 2>/dev/null)"
  done
  seen=$now
  sleep 30
done
