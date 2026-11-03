#!/usr/bin/env bash
# Retime 3d067f7 against its parent c71e993 on one pinned core, and measure
# where the solve's inner loop leaves the cache.
#
#   bash measurements/retime_3d067f7/run.sh [SEED]
#
# Builds settle.rs as an example in a checkout of each commit, and kernel.rs
# and steps.rs in the checkout of 3d067f7. Then runs RUNS of each of the four
# jobs one after another, in an order shuffled with SEED (random when not
# given), each pinned to CORE. Writes raw/, order.txt and summary.txt here.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(git -C "$here" rev-parse --show-toplevel)"
before=c71e993dfe4b77efd45c5fe1762bfdb874e7acdd
after=3d067f7b677a99fac14d9c338d6aa26b3e1951f3
# Six per side, the fewest runs the rule for these timings accepts behind a
# median and an interquartile range.
runs=6
# One core, so the scheduler never moves a run, and the core the timings
# behind 3d067f7 ran on. /proc/interrupts counted one device interrupt on it
# from boot through this run.
core=2
# Two options at 512 to 4096 voters, the panels behind the figures in
# 3d067f7's message, so these medians stand beside them.
settle_args="2 512 1024 2048 4096"
# The two sizes 3d067f7's message compares when it says the solve grows
# faster than n^3.
steps_args="2048 4096"
seed="${1:-$(od -An -N4 -tu4 /dev/urandom | tr -d ' ')}"

work="$(mktemp -d)"
cleanup() {
  for side in before after; do
    git -C "$repo" worktree remove --force "$work/$side" 2>/dev/null || true
  done
  rm -rf "$work"
}
trap cleanup EXIT

for side in before after; do
  git -C "$repo" worktree add --quiet --detach "$work/$side" "${!side}"
  cp "$here/settle.rs" "$work/$side/examples/retime_settle.rs"
done
cp "$here/kernel.rs" "$work/after/examples/retime_kernel.rs"
cp "$here/steps.rs" "$work/after/examples/retime_steps.rs"
(cd "$work/before" && cargo build --quiet --release --example retime_settle)
(cd "$work/after" && cargo build --quiet --release --example retime_settle --example retime_kernel --example retime_steps)

cpuinfo() { grep -m1 "^$1" /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//'; }
cache() { echo "$(cat "/sys/devices/system/cpu/cpu$core/cache/index$1/size") over CPUs $(cat "/sys/devices/system/cpu/cpu$core/cache/index$1/shared_cpu_list")"; }
governor_file="/sys/devices/system/cpu/cpu$core/cpufreq/scaling_governor"
if [[ -r "$governor_file" ]]; then
  governor="$(cat "$governor_file")"
else
  governor="none: the guest has no cpufreq driver for CPU $core, so the host sets the clock"
fi
virt="$(systemd-detect-virt 2>/dev/null || echo none)"
if grep -qw hypervisor /proc/cpuinfo; then
  virt="$virt (systemd-detect-virt), and /proc/cpuinfo sets the hypervisor flag"
fi
machine="$(cpuinfo 'model name'), family $(cpuinfo 'cpu family') model $(cpuinfo 'model'$'\t') stepping $(cpuinfo stepping), $(nproc) CPUs at $(cpuinfo 'cpu MHz') MHz; L2 $(cache 2), L3 $(cache 3) (sysfs cache index2 and index3)"
thp="$(cat /sys/kernel/mm/transparent_hugepage/enabled)"

mkdir -p "$here/raw"
rm -f "$here"/raw/*.txt
jobs="$(python3 - "$seed" "$runs" <<'EOF'
import random, sys
seed, runs = int(sys.argv[1]), int(sys.argv[2])
jobs = [f"{kind} {k}" for kind in ("before", "after", "kernel", "steps") for k in range(1, runs + 1)]
random.Random(seed).shuffle(jobs)
print("\n".join(jobs))
EOF
)"
total="$(wc -l <<<"$jobs")"
{
  echo "# The order run.sh ran its jobs in: Python's random.Random($seed).shuffle over $runs runs of each job."
  echo "| job | kind | run | started UTC | ended UTC |"
  echo "|---:|---|---:|---|---|"
} >"$here/order.txt"

job=0
while read -r kind k; do
  job=$((job + 1))
  case "$kind" in
    before) dir=before commit=$before bin=retime_settle args=$settle_args out="settle-c71e993-$k.txt" source=settle.rs ;;
    after) dir=after commit=$after bin=retime_settle args=$settle_args out="settle-3d067f7-$k.txt" source=settle.rs ;;
    kernel) dir=after commit=$after bin=retime_kernel args="" out="kernel-$k.txt" source=kernel.rs ;;
    steps) dir=after commit=$after bin=retime_steps args=$steps_args out="steps-$k.txt" source=steps.rs ;;
  esac
  command="taskset -c $core target/release/examples/$bin${args:+ $args}"
  started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  {
    echo "# $kind run $k of $runs, job $job of $total in order.txt, at commit $commit"
    echo "# command: $command, in a checkout of that commit with this directory's $source as examples/$bin.rs"
    echo "# rustc: $(cd "$work/$dir" && rustc -V)"
    echo "# machine: $machine"
    echo "# governor: $governor"
    echo "# virtualization: $virt"
    echo "# kernel: $(uname -sr); transparent huge pages: $thp"
    echo "# started $started"
    # shellcheck disable=SC2086
    (cd "$work/$dir" && taskset -c "$core" "target/release/examples/$bin" $args)
  } >"$here/raw/$out"
  echo "| $job | $kind | $k | $started | $(date -u +%Y-%m-%dT%H:%M:%SZ) |" >>"$here/order.txt"
done <<<"$jobs"

python3 "$here/summary.py" >"$here/summary.txt"
