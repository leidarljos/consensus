#!/usr/bin/env python3
"""Medians and interquartile ranges over the raw files run.sh wrote.

    python3 measurements/retime_3d067f7/summary.py > measurements/retime_3d067f7/summary.txt
"""

import re
import statistics
from datetime import datetime
from pathlib import Path

HERE = Path(__file__).resolve().parent
RAW = HERE / "raw"
SIDES = (("c71e993", "before"), ("3d067f7", "after"))
SETTLE = ("build s", "solve s", "iterate s", "exact s", "solve again s")
PHASES = ("scale s", "classes s", "stop init s", "eliminate s", "back s", "total s", "total / n^3 ns")
SHARED = ("# rustc:", "# machine:", "# governor:", "# virtualization:", "# kernel:", "# this process may run on CPUs")


def cells(line: str) -> list[str]:
    return [c.strip() for c in line.strip().strip("|").split("|")]


def table(path: Path, width: int) -> list[list[str]]:
    return [
        c
        for c in map(cells, path.read_text().splitlines())
        if len(c) == width and c[0].isdigit()
    ]


def runs(pattern: str) -> list[Path]:
    paths = sorted(RAW.glob(pattern), key=lambda p: int(p.stem.rsplit("-", 1)[1]))
    assert paths, pattern
    return paths


def spread(xs: list[float], digits: int) -> tuple[str, str]:
    q1, q2, q3 = statistics.quantiles(xs, n=4, method="inclusive")
    return f"{q2:.{digits}f}", f"{q1:.{digits}f} to {q3:.{digits}f}"


def median(xs: list[float]) -> float:
    return statistics.median(xs)


every = sorted(RAW.glob("*.txt"))
shared = {}
for path in every:
    for key in SHARED:
        lines = [l for l in path.read_text().splitlines() if l.startswith(key)]
        assert len(lines) == 1, (path.name, key)
        shared.setdefault(key, set()).add(lines[0])
for key, lines in shared.items():
    assert len(lines) == 1, (key, lines)
seed = re.search(r"Random\((\d+)\)", (HERE / "order.txt").read_text()).group(1)

settle = {}
for short, _ in SIDES:
    for path in runs(f"settle-{short}-*.txt"):
        for row in table(path, 8):
            n, build, solve, iterate, rounds, exact, engine, again = row
            settle.setdefault((short, int(n)), []).append(
                {
                    "build s": float(build),
                    "solve s": float(solve),
                    "iterate s": float(iterate),
                    "exact s": float(exact),
                    "solve again s": float(again),
                    "rounds": int(rounds),
                    "engine": engine,
                }
            )
counts = {len(v) for v in settle.values()}
assert len(counts) == 1, counts
per_side = counts.pop()
voters = sorted({n for _, n in settle})

print(f"# {len(every)} raw files in raw/, run in the order of order.txt (seed {seed}).")
print("# Every file's header agrees on these lines:")
for key in SHARED:
    print(next(iter(shared[key])))
print()
print(f"## Settle, {per_side} runs per commit")
print()
print("# median and interquartile range (inclusive quartiles) over the runs; the ratio is c71e993's median over 3d067f7's.")
print("| voters | measure | c71e993 median | c71e993 IQR | 3d067f7 median | 3d067f7 IQR | ratio of medians |")
print("|---:|---|---:|---:|---:|---:|---:|")
for n in voters:
    for measure in SETTLE:
        b = [r[measure] for r in settle[("c71e993", n)]]
        a = [r[measure] for r in settle[("3d067f7", n)]]
        bm, bi = spread(b, 3)
        am, ai = spread(a, 3)
        ratio = f"{median(b) / median(a):.2f}" if median(a) > 0 else "-"
        print(f"| {n} | {measure} | {bm} | {bi} | {am} | {ai} | {ratio} |")
print()
print("# iterate rounds and exact engine, over every run")
print("| voters | commit | iterate rounds | exact engine |")
print("|---:|---|---|---|")
for n in voters:
    for short, _ in SIDES:
        rs = settle[(short, n)]
        rounds = ", ".join(str(x) for x in sorted({r["rounds"] for r in rs}))
        engines = ", ".join(sorted({r["engine"] for r in rs}))
        print(f"| {n} | {short} | {rounds} | {engines} |")
print()
print("# Runs in which a solve timed alone took longer than a settle that ran the same solve inside it.")
print("| voters | commit | solve over iterate | solve over exact | solve again over iterate | solve again over exact | median solve / solve again |")
print("|---:|---|---:|---:|---:|---:|---:|")
for n in voters:
    if n < 1024:
        continue
    for short, _ in SIDES:
        rs = settle[(short, n)]
        over = lambda solve, whole: sum(r[solve] > r[whole] for r in rs)
        paired = median([r["solve s"] / r["solve again s"] for r in rs])
        print(
            f"| {n} | {short} | {over('solve s', 'iterate s')} of {len(rs)} | {over('solve s', 'exact s')} of {len(rs)} "
            f"| {over('solve again s', 'iterate s')} of {len(rs)} | {over('solve again s', 'exact s')} of {len(rs)} | {paired:.3f} |"
        )

print()
print("# Wall seconds per settle job, from order.txt, against the seconds it timed. The rest is the panels it built untimed, and any time the machine was paused: its clocks stop while it is, so its timings leave the pause out.")
print("| job | commit | run | wall s | timed s | wall - timed s |")
print("|---:|---|---:|---:|---:|---:|")
stamp = "%Y-%m-%dT%H:%M:%SZ"
commit_of = {side: short for short, side in SIDES}
for job, kind, k, started, ended in table(HERE / "order.txt", 5):
    if kind not in commit_of:
        continue
    short = commit_of[kind]
    wall = (datetime.strptime(ended, stamp) - datetime.strptime(started, stamp)).total_seconds()
    timed = sum(
        float(c[i]) for c in table(RAW / f"settle-{short}-{k}.txt", 8) for i in (1, 2, 3, 5, 7)
    )
    print(f"| {job} | {short} | {k} | {wall:.0f} | {timed:.0f} | {wall - timed:.0f} |")

kernel = {}
for path in runs("kernel-*.txt"):
    for mib, _, _, ns, gbs in table(path, 5):
        kernel.setdefault(int(mib), []).append((float(ns), float(gbs)))
kernel_runs = {len(v) for v in kernel.values()}
assert len(kernel_runs) == 1, kernel_runs
print()
print(f"## Kernel, {kernel_runs.pop()} runs")
print()
print("| working set MiB | ns per entry median | ns per entry IQR | GB/s moved median | GB/s moved IQR |")
print("|---:|---:|---:|---:|---:|")
for mib in sorted(kernel):
    ns = [x for x, _ in kernel[mib]]
    gbs = [y for _, y in kernel[mib]]
    nm, ni = spread(ns, 3)
    gm, gi = spread(gbs, 1)
    print(f"| {mib} | {nm} | {ni} | {gm} | {gi} |")

phases, eighths = {}, {}
for path in runs("steps-*.txt"):
    for row in table(path, 8):
        phases.setdefault(int(row[0]), []).append([float(x) for x in row[1:]])
    for n, eighth, mib, ns in table(path, 4):
        eighths.setdefault((int(n), eighth), []).append((int(mib), float(ns)))
steps_runs = {len(v) for v in phases.values()} | {len(v) for v in eighths.values()}
assert len(steps_runs) == 1, steps_runs
print()
print(f"## Solve stages at 3d067f7, {steps_runs.pop()} runs")
print()
print("| voters | stage | median | IQR |")
print("|---:|---|---:|---:|")
for n in sorted(phases):
    for k, stage in enumerate(PHASES):
        m, i = spread([r[k] for r in phases[n]], 3)
        print(f"| {n} | {stage} | {m} | {i} |")
print()
print("| voters | eighth | working set MiB | ns per entry median | ns per entry IQR |")
print("|---:|---|---:|---:|---:|")
for n, eighth in sorted(eighths):
    rs = eighths[(n, eighth)]
    mibs = {m for m, _ in rs}
    assert len(mibs) == 1, (n, eighth, mibs)
    m, i = spread([x for _, x in rs], 3)
    print(f"| {n} | {eighth} | {mibs.pop()} | {m} | {i} |")
