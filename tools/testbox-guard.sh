#!/bin/sh
# Runs a job script on testbox with a hard timeout and a stall kill, then writes its exit code.
# usage (in the job dir): sh tools/testbox-guard.sh <script> <log> <done> <timeout_s> <stall_s>
# Exit codes written to <done>: the job's own, or 124 = timed out, 125 = stalled (no log output and
# no CPU use for <stall_s>; a long silent rustc compile counts as activity, a deadlocked bot doesn't).
# A hung job is killed with its whole process group, so it can't hold a BDS or the box.
script=$1 log=$2 done=$3 timeout=$4 stall=$5
setsid sh "$script" >"$log" 2>&1 </dev/null &
pid=$!
start=$(date +%s)
cpu() { ps -s "$pid" -o times= 2>/dev/null | awk '{s += $1} END {print s + 0}'; }
last_cpu=$(cpu) busy=$start
while kill -0 "$pid" 2>/dev/null; do
  sleep 5
  now=$(date +%s)
  c=$(cpu)
  # The sum drops when a busy child (rustc) exits, so re-base on any decrease.
  [ "$c" -lt "$last_cpu" ] && last_cpu=$c
  [ $(( c - last_cpu )) -ge 2 ] && { busy=$now; last_cpu=$c; }
  seen=$(stat -c %Y "$log")
  [ "$busy" -gt "$seen" ] && seen=$busy
  idle=$(( now - seen ))
  if [ $(( now - start )) -gt "$timeout" ]; then why="timed out after ${timeout}s"; code=124
  elif [ "$idle" -gt "$stall" ]; then why="stalled: no output for ${stall}s"; code=125
  else continue; fi
  echo "testbox-guard: $why; killing the job" >>"$log"
  # dash's kill takes no `--`; a negative pid signals the job's whole process group (setsid made it one).
  kill -TERM "-$pid"; sleep 5; kill -KILL "-$pid" 2>/dev/null
  echo "$code" >"$done"
  exit 0
done
wait "$pid"
echo $? >"$done"
