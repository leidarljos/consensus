"""The settle's and the gate's constants, derived with SymPy.

Run: python3 derive/sympy/constants.py   (rewrites derive/golden/constants.json;
golden.py runs it too)

Each section derives one constant from what it is for and checks the value
the crate holds. tests/derived_constants.rs holds the crate's constants,
Opts::default() and the correlation command's defaults to the file this
writes.
"""

import itertools
import json
import math
import pathlib
import sys

import sympy as sp

R = sp.Rational
EPS = R(1, 2**52)  # f64::EPSILON, the spacing of binary64 at one
U = EPS / 2  # unit roundoff of binary64
LEVEL = R(1, 20)  # correlation::INDEPENDENCE_LEVEL, Fisher's one in twenty


def report(ok, label):
    print(f"{'ok ' if ok else 'FAIL'} {label}")
    return bool(ok)


def gamma(k):
    """Higham's gamma: the relative error of a k-term dot product."""
    return k * U / (1 - k * U)


def trust_floor(results):
    # No record length in the crate fixes the floor, so binary64 does: the
    # floor is the least weight w for which 1 + w is still a double above
    # one, so that a floored voter beside the best, who weighs one, moves
    # their sum. Doubles in [1, 2) are 1 + k EPSILON, so that w is EPSILON.
    # 1 - w must be a double below one too, or the clamped log odds are
    # infinite. Python's float is binary64, so the checks run on it.
    eps = sys.float_info.epsilon
    results.append(report(R(eps) == EPS == R(math.nextafter(1.0, 2.0)) - 1,
                          "EPSILON = 2^-52 is the spacing of doubles at one"))
    results.append(report(1.0 + eps / 2 == 1.0, "1 + 2^-53 rounds back to one, so half of it is lost"))
    results.append(report(R(1.0 + eps) == 1 + EPS and R(1.0 - eps) == 1 - EPS,
                          "1 + 2^-52 and 1 - 2^-52 are both doubles, exactly"))
    end = math.log((1.0 - eps) / eps)
    results.append(report(math.isfinite(end) and abs(end - float(sp.log(2**52 - 1))) < 1e-12,
                          f"the clamped log odds end at ln(2^52 - 1) = {end:.4f}"))
    # log_odds holds its floor at the unit roundoff or above: the least power
    # of two whose complement is a double below one.
    u = eps / 2
    results.append(report(1.0 - u < 1.0 and 1.0 - u / 2 == 1.0,
                          "1 - 2^-53 is below one, and 1 - 2^-54 rounds to one"))
    return {"value": str(EPS), "log_odds_end": f"{end:.12f}", "log_odds_floor": str(U)}


def independence_level(results, top=40):
    # The gate is Fisher's one-sided exact test. Of m shared items the
    # voters got r and c right; when they err apart, the count A they both
    # got right is hypergeometric given r and c, whatever their accuracies.
    # The test passes a table read a when P(A >= a) <= LEVEL. That tail falls
    # as a rises, so the tables it passes are those from some a* on, and
    # they turn up P(A >= a*) <= LEVEL of the time. Any accuracies mix the
    # margins, so the rate stays under LEVEL too, at every count of items.
    # Every margin up to `top` items is checked here in rationals.
    sizes, least, holds, positive = {}, None, True, True
    for m in range(2, top + 1):
        worst, fewest = R(0), R(1)
        for r, c in itertools.product(range(m + 1), repeat=2):
            lo, hi = max(0, r + c - m), min(r, c)
            den = math.comb(m, c)
            tail, size = R(0), R(0)
            for a in range(hi, lo - 1, -1):
                at = R(math.comb(r, a) * math.comb(m - r, c - a), den)
                tail += at
                fewest = min(fewest, tail)
                if tail <= LEVEL:
                    size += at
                    positive &= a * m > r * c
            worst = max(worst, size)
        holds &= worst <= LEVEL and fewest == R(1, math.comb(m, m // 2))
        if least is None and worst > 0:
            least = m
        if 5 <= m <= 12:
            sizes[str(m)] = str(worst)
    results.append(report(holds, f"on every margin of 2 to {top} items the test passes voters who "
                                 f"err apart at most {LEVEL} of the time, and its least tail is "
                                 f"1 / C(m, m / 2)"))
    results.append(report(positive, "every table it passes reads a positive correlation"))
    only = [(r, c, a) for r, c in itertools.product(range(7), repeat=2)
            for a in range(max(0, r + c - 6), min(r, c) + 1)
            if sum(R(math.comb(r, x) * math.comb(6 - r, c - x), math.comb(6, c))
                   for x in range(a, min(r, c) + 1)) <= LEVEL]
    results.append(report(least == 6 and only == [(3, 3, 3)],
                          f"it passes nothing below {least} items, and on six only clones right "
                          f"on three"))
    # The crate holds the level as a double, which the test meets exactly. A
    # tail T / C(m, c) above 1/20 is above it by at least 1 / (20 C(m, c)),
    # so below `exact` items no tail falls between 1/20 and the double.
    above = R(float(LEVEL)) - LEVEL
    exact = next(m for m in itertools.count(1) if R(1, 20 * math.comb(m, m // 2)) <= above)
    results.append(report(0 < above < R(3, 10**18) and exact == 58,
                          f"the double 0.05 is above 1/20 by {float(above):.3g}, which no tail "
                          f"of fewer than {exact} items falls within"))
    return {"value": str(LEVEL), "f64": repr(float(LEVEL)), "least_shared": least,
            "exact_below": exact, "largest_size": sizes}


def contraction_bound(results):
    # Nothing in the crate bounds q = max s_i: the susceptibility defaults to
    # one, and the self weight is a raw entry before its row is normalised.
    # Banach's estimate |x_t - x*| <= q / (1 - q) |x_t - x_(t-1)| has a factor
    # of at most one, so the last step bounds the distance by itself, exactly
    # while q <= 1/2.
    q = sp.Symbol("q", positive=True)
    factor = q / (1 - q)
    edge = sp.solve(sp.Eq(factor, 1), q)
    results.append(report(edge == [R(1, 2)], f"q / (1 - q) is one at q = {edge[0]}"))
    results.append(report(sp.simplify(sp.diff(factor, q) - 1 / (1 - q)**2) == 0,
                          "and rises with q, as its derivative 1 / (1 - q)^2 is positive"))
    return {"value": str(edge[0])}


def supported_voters(results):
    # Outcome::sealed reads two shares within 2 residual + 4 n EPSILON as a
    # tie. A one-ballot margin 1 / n clears the rounding part only while
    # n^2 < 1 / (4 EPSILON).
    edge = sp.sqrt(1 / (4 * EPS))
    results.append(report(edge == 2**25, f"1 / n > 4 n EPSILON while n < {edge} = 2^25"))
    n = 2**25 - 1
    results.append(report(R(1, n) > 4 * n * EPS and R(1, n + 1) <= 4 * (n + 1) * EPS,
                          f"{n} voters is the largest panel that can name a one-ballot winner"))
    return {"value": n}


def tolerance(results, q, voters):
    # Binary64 adds gamma(n + 2) / (1 - q) to the bound settle_with stops on
    # (derive/sollya/rounding.sollya). The tolerance is twice that at the
    # largest panel and contraction, so rounding takes at most half of it and
    # truncation has the other half.
    tol = 2 * gamma(voters + 2) / (1 - q)
    results.append(report(tol == R(4 * (2**25 + 1), 2**53 - 2**25 - 1),
                          f"tol = 4 gamma(2^25 + 1) = {tol}"))
    # Opts::default computes it in f64: k u exactly, one rounding in the
    # quotient, and the doubling and the halving exactly. That is tol
    # rounded once.
    ku = float(voters + 2) * sys.float_info.epsilon / 2.0
    crate = 2.0 * (ku / (1.0 - ku)) / (1.0 - float(q))
    nearest = tol.p / tol.q
    results.append(report(crate == nearest, f"the crate's f64 is tol rounded once, {crate!r}"))
    return {"value": str(tol), "f64": repr(crate)}


def round_cap(results, q, tol):
    # At or under the bound the truncation in settle_with's bound is at most
    # q^t after t rounds. The cap is the count fj_rounds_to_tol gives,
    # 1 + ceil(ln tol / ln q), and it is the first that puts q^t under the
    # half of tol rounding leaves.
    cap = 1 + int(sp.ceiling(sp.log(tol) / sp.log(q)))
    results.append(report(q**cap < tol / 2 < q**(cap - 1),
                          f"{q}^{cap} < tol / 2 < {q}^{cap - 1}: {cap} rounds"))
    crate = 1 + math.ceil(math.log(tol.p / tol.q) / math.log(float(q)))
    results.append(report(crate == cap, f"fj_rounds_to_tol in f64 gives {crate} too"))
    return {"value": cap}


def gauge(results):
    # settle_energy writes each opinion as a softmax of free logits. The
    # softmax is unchanged by adding one number to a row, so its Jacobian
    # J = diag(x) - x x^T has J 1 = 0, and the energy's gradient in the
    # logits, J dE/dx, sums to zero on every row. The start logits sum to
    # zero too, so a descent stays on that plane wherever the box does not
    # clip, and a penalty on a row's summed logits has nothing to fix: its
    # weight is zero.
    for m in (2, 3, 4):
        z = sp.symbols(f"z0:{m}")
        e = [sp.exp(v) for v in z]
        x = [v / sum(e) for v in e]
        jac = sp.Matrix(m, m, lambda j, k: sp.diff(x[j], z[k]))
        results.append(report(sp.simplify(jac * sp.ones(m, 1)) == sp.zeros(m, 1),
                              f"{m} options: the softmax Jacobian takes 1 to 0"))
        results.append(report(sp.simplify(jac - jac.T) == sp.zeros(m, m),
                              f"{m} options: it is symmetric, so each row's gradient sums to zero"))
        start = [4 * R(m - 1, m)] + [R(-4, m)] * (m - 1)
        results.append(report(sum(start) == 0, f"{m} options: the start logits sum to zero"))


def main():
    results = []
    out = {
        "trust_floor": trust_floor(results),
        "independence_level": independence_level(results),
        "contraction_bound": contraction_bound(results),
        "supported_voters": supported_voters(results),
    }
    q = R(out["contraction_bound"]["value"])
    out["tol"] = tolerance(results, q, out["supported_voters"]["value"])
    out["max_iter"] = round_cap(results, q, R(out["tol"]["value"]))
    gauge(results)
    print()
    if not all(results):
        print(f"{results.count(False)} of {len(results)} checks failed")
        return 1
    path = pathlib.Path(__file__).resolve().parent.parent / "golden" / "constants.json"
    path.write_text(json.dumps(out, indent=1) + "\n")
    print(f"all {len(results)} checks hold; wrote {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
