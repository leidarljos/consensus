"""What the Friedkin-Johnsen settle computes, derived and checked with SymPy.

Run: python3 derive/sympy/fj.py   (exit 0 when every identity holds)

The settle iterates x(t+1) = L W x(t) + (I - L) x0 over a row-stochastic
trust matrix W and a diagonal of susceptibilities L = diag(s). Each identity
below is stated, derived symbolically, and asserted.
"""

import sys

import sympy as sp


def row_stochastic(n, name="w"):
    """A symbolic n x n matrix whose rows sum to one (last column solved)."""
    rows = []
    free = []
    for i in range(n):
        row = [sp.Symbol(f"{name}{i}{j}", nonnegative=True) for j in range(n - 1)]
        free.extend(row)
        rows.append(row + [1 - sum(row)])
    return sp.Matrix(rows), free


def check(label, expr):
    expr = sp.simplify(expr)
    ok = expr == 0 if not isinstance(expr, sp.MatrixBase) else expr == sp.zeros(*expr.shape)
    print(f"{'ok ' if ok else 'FAIL'} {label}")
    if not ok:
        print("     residual:", expr)
    return ok


def main():
    results = []
    n = 3
    one = sp.ones(n, 1)
    W, _ = row_stochastic(n)
    s = sp.symbols(f"s0:{n}", positive=True)
    L = sp.diag(*s)
    I = sp.eye(n)

    # 1. The fixed point is x* = P x0 with P = (I - L W)^-1 (I - L), and P is
    #    row-stochastic: (I - L W) 1 = 1 - L 1 = (I - L) 1, so P 1 = 1.
    results.append(check("(I - L W) 1 = (I - L) 1", (I - L * W) * one - (I - L) * one))
    P = (I - L * W).inv() * (I - L)
    results.append(check("P 1 = 1 (P row-stochastic)", P * one - one))

    # 2. The shares are a weighted vote. shares = (1/n) 1^T P X0 and X0's rows
    #    are ballot indicators, so share_k = sum over voters of option k of c_i
    #    with c = (1/n) P^T 1, the social power (Friedkin 1991,
    #    doi:10.1086/229694); sum c = 1.
    c = (P.T * one) / n
    results.append(check("sum of social power = 1", sum(c) - 1))

    # 3. The rows `learn` writes: every voter gives voter j the same weight
    #    w_j, and itself a constant self-weight sw. The DeGroot limit is the
    #    left eigenvector pi with pi_j proportional to w_j (S + sw - w_j),
    #    S = sum w: not proportional to w_j, so the settle compresses the
    #    log-odds weights it was handed.
    w = sp.symbols(f"w0:{n}", positive=True)
    sw = sp.Symbol("sw", positive=True)
    S = sum(w)
    Wl = sp.Matrix(n, n, lambda i, j: (sw if i == j else w[j]) / (S - w[i] + sw))
    results.append(check("learned rows are row-stochastic", Wl * one - one))
    pi = sp.Matrix([w[j] * (S + sw - w[j]) for j in range(n)])
    pi = pi / sum(pi)
    results.append(check("pi^T W = pi^T for pi_j ~ w_j (S + sw - w_j)", (pi.T * Wl - pi.T).T))

    # 4. Earned self-trust: fill the diagonal with the voter's own inbound
    #    weight. Every row is then w / S, W = 1 w^T / S is rank one, the
    #    DeGroot limit is pi = w / S in one round, and the settle is the
    #    weighted majority with weights w: Nitzan-Paroush exactly when w are
    #    log odds (doi:10.2307/2526438).
    We = sp.Matrix(n, n, lambda i, j: w[j] / S)
    results.append(check("earned self-trust: W = 1 w^T / S", We - one * sp.Matrix([w]) / S))
    results.append(check("earned self-trust: W^2 = W (one round)", We * We - We))

    # 5. With earned self-trust and anchors, Sherman-Morrison gives the
    #    social power in closed form:
    #      c_j = (1 - s_j) / n * (1 + w_j sum_i s_i / (S - sum_i w_i s_i)).
    #    With one susceptibility s for all, c = (1 - s) / n + s w / S: the
    #    settle is the mixture (1 - s) count + s accuracy-weighted vote.
    Pe = (I - L * We).inv() * (I - L)
    ce = (Pe.T * one) / n
    sws = sum(w[i] * s[i] for i in range(n))
    closed = sp.Matrix(
        [(1 - s[j]) / n * (1 + w[j] * sum(s) / (S - sws)) for j in range(n)]
    )
    results.append(check("earned self-trust social power, closed form", ce - closed))
    s_all = sp.Symbol("s", positive=True)
    uniform = ce.subs({si: s_all for si in s})
    mix = sp.Matrix([(1 - s_all) / n + s_all * w[j] / S for j in range(n)])
    results.append(check("uniform anchor: c = (1-s)/n + s w/S", uniform - mix))

    # 6. Convergence rate of the learned rows with equal weights w and a
    #    constant self-weight sw: W = a 1 1^T + (b - a) I with
    #    a = w / D, b = sw / D, D = (n - 1) w + sw, so the second eigenvalue
    #    is (sw - w) / D: zero, one round, exactly when sw = w.
    wq = sp.Symbol("wq", positive=True)
    D = (n - 1) * wq + sw
    Wq = sp.Matrix(n, n, lambda i, j: (sw if i == j else wq) / D)
    lam = sp.Symbol("lam")
    charpoly = sp.factor((Wq - lam * I).det())
    lam2 = (sw - wq) / D
    results.append(check("second eigenvalue (sw - w) / ((n-1) w + sw)", charpoly.subs(lam, lam2)))

    # 7. The a-posteriori bound the iteration stops on. With q = max s_i < 1
    #    the map is a q-contraction in the sup norm (each row of W averages),
    #    so |x_t - x*| <= q / (1 - q) |x_t - x_(t-1)|: the tail of the
    #    geometric series sum_{k>=1} q^k. The Lean file proves the bound; here
    #    the series and the round count it implies.
    q, k = sp.symbols("q k", positive=True)
    tail = sp.summation(q**k, (k, 1, sp.oo))
    results.append(check("sum_{k>=1} q^k = q/(1-q) for |q|<1", sp.piecewise_fold(tail).args[0][0] - q / (1 - q)))

    print()
    if all(results):
        print(f"all {len(results)} identities hold")
        return 0
    print(f"{results.count(False)} of {len(results)} identities failed")
    return 1


if __name__ == "__main__":
    sys.exit(main())
