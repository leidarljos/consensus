#!/usr/bin/env python3
"""Verify the Friedkin-Johnsen derivation against the shipped binary.

For random small panels this script checks the four claims made in
``docs/orgmode/derivation.org``:

1. Closed form: ``x* = (I - S W)^{-1} (I - S) x0`` satisfies the fixed
   point equation, exactly (SymPy rationals) and in float.
2. Neumann series: partial sums of ``(S W)^t`` converge to the closed
   form -- the term-by-term limit the derivation page cites.
3. Contraction: successive sup-norm differences shrink by at most
   ``max s_i`` per round, the bound ``fj_contraction_bound`` returns.
4. Round count: the iteration reaches ``tol`` within
   ``1 + ceil(ln(tol) / ln(bound))`` rounds, the count
   ``fj_rounds_to_tol`` returns -- and the shipped ``ljos-consensus``
   binary agrees with the closed form end to end.
5. Interval enclosure (Sollya-style): the same settle replayed in
   outward-rounded ``mpmath.iv`` intervals contains the float stop and
   the closed form, with a small final width -- the stop at ``tol``
   sits above the rounding floor.

Usage: ``cargo build`` first (or let this script build), then
``python3 scripts/verify_fj.py``. Exits non-zero on any failure.
"""

from __future__ import annotations

import json
import math
import os
import random
import subprocess
import sys
from pathlib import Path

import sympy as sp

ROOT = Path(__file__).resolve().parent.parent
TOL = 1e-9


def build_case(rng: random.Random, n: int, m: int):
    agents = [chr(ord("a") + i) for i in range(n)]
    options = [f"opt{k}" for k in range(m)]
    # Row-stochastic influence with a defensible diagonal, as floats.
    W = []
    for _ in range(n):
        row = [rng.random() + 0.05 for _ in range(n)]
        s = sum(row)
        W.append([v / s for v in row])
    for i in range(n):
        W[i][i] += 0.1
        s = sum(W[i])
        W[i] = [v / s for v in W[i]]
    # Every voter keeps some anchor, so (I - S W) is invertible.
    sus = {a: 0.05 + 0.9 * rng.random() for a in agents}
    choices = {a: rng.choice(options) for a in agents}
    return agents, options, W, sus, choices


def fj_step(W, sus, anchor, x):
    """One Friedkin-Johnsen round from x, anchored at the ballots."""
    n, m = len(W), len(anchor[0])
    nxt = []
    for i in range(n):
        s = sus[i]
        row = []
        for k in range(m):
            heard = sum(W[i][j] * x[j][k] for j in range(n))
            row.append((1.0 - s) * anchor[i][k] + s * heard)
        nxt.append(row)
    return nxt


def iterate(W, sus, anchor, steps):
    n, m = len(W), len(anchor[0])
    x = [row[:] for row in anchor]
    for _ in range(steps):
        x = fj_step(W, sus, anchor, x)
    return x


def sup_diff(a, b):
    return max(abs(p - q) for ra, rb in zip(a, b) for p, q in zip(ra, rb))


def closed_form_float(W, sus_list, x0):
    n, m = len(W), len(x0[0])
    SW = [[sus_list[i] * W[i][j] for j in range(n)] for i in range(n)]
    A = [[(1.0 if i == j else 0.0) - SW[i][j] for j in range(n)] for i in range(n)]
    B = [[(1.0 - sus_list[i]) * x0[i][k] for k in range(m)] for i in range(n)]
    M = sp.Matrix(A)
    rhs = sp.Matrix(B)
    sol = M.LUsolve(rhs)
    return [[float(sol[i * m + k]) for k in range(m)] for i in range(n)]


def run_binary(binp: Path, ballots: str, trust: str, sus_of: str) -> dict:
    out = subprocess.run(
        [str(binp), "settle", "--ballots", ballots, "--trust", trust,
         "--susceptibility-of", sus_of, "--max-iter", "500", "--tol", str(TOL)],
        capture_output=True, text=True, check=True,
    )
    return json.loads(out.stdout)


def main() -> int:
    seed0 = int(os.environ.get("VERIFY_FJ_SEED", "0"))
    failures = 0
    checks = 0

    def check(name: str, ok: bool, detail: str = ""):
        nonlocal failures, checks
        checks += 1
        print(("PASS " if ok else "FAIL ") + name + (f" -- {detail}" if detail else ""))
        if not ok:
            failures += 1

    # Build the binary once so the end-to-end check exercises it.
    build = subprocess.run(["cargo", "build", "--locked"], cwd=ROOT,
                           capture_output=True, text=True)
    if build.returncode != 0:
        print("FAIL cargo build --locked\n" + build.stderr[-2000:])
        return 1
    binp = ROOT / "target" / "debug" / "ljos-consensus"
    check("cargo build --locked", binp.is_file())

    # Exact rational check on one small panel: the fixed-point equation
    # holds symbolically, not just numerically.
    Wq = [[sp.Rational(1, 2), sp.Rational(1, 2)],
          [sp.Rational(1, 4), sp.Rational(3, 4)]]
    sq = [sp.Rational(1, 3), sp.Rational(2, 3)]
    x0q = [[sp.Integer(1), sp.Integer(0)], [sp.Integer(0), sp.Integer(1)]]
    n = 2
    SWq = sp.Matrix([[sq[i] * Wq[i][j] for j in range(n)] for i in range(n)])
    Aq = sp.eye(n) - SWq
    Bq = sp.Matrix([[(1 - sq[i]) * x0q[i][k] for k in range(2)] for i in range(n)])
    xstar_q = Aq.LUsolve(Bq)
    check("exact fixed point (rationals)", Aq * xstar_q == Bq,
          f"x*={[[str(v) for v in row] for row in xstar_q.tolist()]}")

    for seed in range(seed0, seed0 + 8):
        rng = random.Random(seed)
        n = [2, 3, 5][seed % 3]
        m = [2, 3][seed % 2]
        agents, options, W, sus, choices = build_case(rng, n, m)
        sus_list = [sus[a] for a in agents]
        bound = max(sus_list)
        x0 = [[1.0 if choices[a] == o else 0.0 for o in options] for a in agents]
        tag = f"seed={seed} n={n} m={m}"

        # 1. Closed form satisfies the fixed point in float.
        xs = closed_form_float(W, sus_list, x0)
        SW = [[sus_list[i] * W[i][j] for j in range(n)] for i in range(n)]
        resid = max(
            abs(sum(((1.0 if i == j else 0.0) - SW[i][j]) * xs[j][k] for j in range(n))
                - (1.0 - sus_list[i]) * x0[i][k])
            for i in range(n) for k in range(m))
        check(f"closed form residual [{tag}]", resid < 1e-9, f"res={resid:.2e}")

        # 2. Neumann partial sums converge to the closed form.
        acc = [[(1.0 - sus_list[i]) * x0[i][k] for k in range(m)] for i in range(n)]
        term = [row[:] for row in acc]
        for _ in range(200):
            term = [[sum(SW[i][j] * term[j][k] for j in range(n))
                     for k in range(m)] for i in range(n)]
            acc = [[acc[i][k] + term[i][k] for k in range(m)] for i in range(n)]
        gap = sup_diff(acc, xs)
        check(f"neumann series limit [{tag}]", gap < 1e-6, f"gap={gap:.2e}")

        # 3. Contraction: successive diffs shrink by at most the bound.
        x_prev, x_cur = x0, fj_step(W, sus_list, x0, x0)
        d_prev = sup_diff(x_cur, x_prev)
        ok = True
        for _ in range(60):
            x_nxt = fj_step(W, sus_list, x0, x_cur)
            d_cur = sup_diff(x_nxt, x_cur)
            if d_cur > bound * d_prev + 1e-12:
                ok = False
                break
            x_prev, x_cur, d_prev = x_cur, x_nxt, d_cur
        check(f"contraction by bound={bound:.3f} [{tag}]", ok)

        # 4a. Round-count prediction covers the iteration.
        predicted = 1 + math.ceil(math.log(TOL) / math.log(bound))
        x, rounds, settled = x0, 0, False
        for r in range(1, 501):
            nxt = fj_step(W, sus_list, x0, x)
            rounds = r
            if sup_diff(nxt, x) < TOL:
                x = nxt
                settled = True
                break
            x = nxt
        check(f"rounds {rounds} <= predicted {predicted} [{tag}]",
              settled and rounds <= predicted)

        # 4b. The shipped binary agrees with the closed form end to end.
        ballots = json.dumps([{"agent": a, "choice": choices[a]} for a in agents])
        trust = json.dumps([
            {"from": a, "to": b, "weight": W[i][j]}
            for i, a in enumerate(agents) for j, b in enumerate(agents)
        ])
        sus_of = json.dumps(sus)
        outcome = run_binary(binp, ballots, trust, sus_of)
        shares_cf = [sum(xs[i][k] for i in range(n)) / n for k in range(m)]
        # The binary lists only options some ballot chose; an unchosen
        # option keeps zero mass under the anchored step.
        got = dict(zip(outcome["options"], outcome["shares"]))
        want = dict(zip(options, shares_cf))
        agree = all(abs(got[o] - want[o]) < 1e-6 for o in got)
        agree = agree and all(want[o] < 1e-9 for o in options if o not in got)
        check(f"binary agrees with closed form [{tag}]",
              agree and outcome.get("converged", outcome["settled"]),
              f"shares={outcome['shares']}")

    interval_case(check)

    print(f"\n{checks - failures}/{checks} checks passed")
    return 1 if failures else 0

def interval_case(check):
    """Sollya-style enclosure: the FJ iterate in outward-rounded intervals.

    Replays one anchored settle (seed 1: three voters, three options, a
    real split) with every value an ``mpmath.iv`` interval, so each step
    encloses all rounding of the float evaluation. Three things must hold:
    the float iterate at the stopping round lies inside the enclosure
    (the stop at ``tol`` is above the rounding floor), the closed form
    lies inside the final enclosure (both name the same point), and the
    final width is small (contraction squeezes rounding instead of
    accumulating it). This is the interval half of the floating-point
    budget on the derivation page; a Sollya certificate of the compiled
    Rust evaluation order remains the open half.
    """
    from mpmath import iv

    rng = random.Random(1)
    n, m = 3, 3
    agents, options, Wf, susf, choices = build_case(rng, n, m)
    sus = [susf[a] for a in agents]
    x0f = [[1.0 if choices[a] == o else 0.0 for o in options] for a in agents]
    W = [[iv.mpf(v) for v in row] for row in Wf]
    S = [iv.mpf(s) for s in sus]
    A = [[iv.mpf(v) for v in row] for row in x0f]
    X = [row[:] for row in A]
    xf = [row[:] for row in x0f]
    one = iv.mpf(1)
    stopped_at = None
    inside_at_stop = False
    for t in range(1, 301):
        Xn = []
        for i in range(n):
            row = []
            for k in range(m):
                heard = iv.mpf(0)
                for j in range(n):
                    heard += W[i][j] * X[j][k]
                row.append((one - S[i]) * A[i][k] + S[i] * heard)
            Xn.append(row)
        X = Xn
        xf = fj_step(Wf, sus, x0f, xf)
        prev = xf_prev if t > 1 else x0f
        if stopped_at is None and sup_diff(xf, prev) < TOL:
            stopped_at = t
            inside_at_stop = all(
                X[i][k].a <= xf[i][k] <= X[i][k].b
                for i in range(n) for k in range(m))
        xf_prev = [row[:] for row in xf]
    xs = closed_form_float(Wf, sus, x0f)
    inside_cf = all(
        X[i][k].a <= xs[i][k] <= X[i][k].b for i in range(n) for k in range(m))
    width = max(float(X[i][k].b - X[i][k].a) for i in range(n) for k in range(m))
    check("interval enclosure contains float stop",
          stopped_at is not None and inside_at_stop, f"round={stopped_at}")
    check("interval enclosure contains closed form", inside_cf)
    check("interval enclosure width is small", width < 1e-12, f"width={width:.2e}")


if __name__ == "__main__":
    raise SystemExit(main())
