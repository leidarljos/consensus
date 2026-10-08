"""Correlated voters, derived and checked with SymPy.

Run: python3 derive/sympy/jury.py   (exit 0 when every identity holds)

Personas answered by one model share its errors (Kim et al. 2025,
doi:10.48550/arXiv.2506.07962; Chen et al. 2024,
doi:10.48550/arXiv.2403.02419). The jury theorem assumes independence
(Condorcet 1785; Grofman, Owen and Feld 1983, doi:10.1007/BF00125672); Ladha
(doi:10.2307/2111584) and Dietrich and Spiekermann (doi:10.1093/mind/fzt074)
show what a shared cause does to it. This file derives the quantities the
settle uses.
"""

import sys

import sympy as sp


def check(label, expr):
    expr = sp.simplify(expr)
    ok = expr == 0 if not isinstance(expr, sp.MatrixBase) else expr == sp.zeros(*expr.shape)
    print(f"{'ok ' if ok else 'FAIL'} {label}")
    if not ok:
        print("     residual:", expr)
    return ok


def main():
    results = []
    p, rho = sp.symbols("p rho", positive=True)
    n, k = sp.symbols("n k", positive=True, integer=True)

    # 1. n exchangeable voters are each right with probability p, and any two
    # have correlation rho in their correctness. The count of right ballots
    # has variance n p (1-p) (1 + (n-1) rho). Their share is as noisy as the
    # share of n_eff = n / (1 + (n-1) rho) independent voters.
    var_one = p * (1 - p)
    var_sum = n * var_one + n * (n - 1) * rho * var_one
    n_eff = n / (1 + (n - 1) * rho)
    results.append(check("Var(count) = n p(1-p) (1 + (n-1) rho)", var_sum - n * var_one * (1 + (n - 1) * rho)))
    results.append(check("n_eff p(1-p) n_eff-variance matches: n_eff = n/(1+(n-1)rho)",
                         sp.simplify(n**2 * var_one / var_sum) - n_eff))
    results.append(check("n_eff -> 1/rho as n -> oo", sp.limit(n_eff, n, sp.oo) - 1 / rho))

    # 2. The weights. The least-variance combination of unbiased signals
    # weighs them in proportion to Sigma^-1 1 (Bates and Granger 1969,
    # doi:10.2307/3008764). Sherman-Morrison solves it for a cluster of k
    # voters of equal strength and equicorrelation rho:
    #     Sigma^-1 1 = 1 / (1 + (k-1) rho) 1.
    # A member's weight is divided by 1 + (k-1) rho, and the cluster is worth
    # k / (1 + (k-1) rho) members; as rho -> 1 it counts as one voter.
    kk = 4
    Sigma = (1 - rho) * sp.eye(kk) + rho * sp.ones(kk, kk)
    weights = Sigma.inv() * sp.ones(kk, 1)
    results.append(check("Sigma^-1 1 = 1/(1+(k-1)rho) for k=4",
                         weights - sp.ones(kk, 1) / (1 + (kk - 1) * rho)))
    total = sp.Rational(kk) / (1 + (kk - 1) * rho)
    results.append(check("cluster of 4 at rho=1 weighs as one voter", total.subs(rho, 1) - 1))

    # A cluster beside an independent voter: Sigma = blockdiag(cluster, 1). An
    # independent voter keeps weight 1; each cluster member gets
    # 1/(1+(k-1)rho).
    Sigma2 = sp.diag(Sigma, sp.Matrix([[1]]))
    w2 = Sigma2.inv() * sp.ones(kk + 1, 1)
    results.append(check("independent voter beside a cluster keeps its weight", w2[kk] - 1))

    # 3. The jury theorem with a shared cause saturates. Model the shared
    # cause as a latent accuracy theta ~ Beta(a, b) with mean p and
    # correlation rho = 1 / (a + b + 1): the beta-binomial. As n grows the
    # majority is right with probability P(theta > 1/2), not 1.
    a = p * (1 / rho - 1)
    b = (1 - p) * (1 / rho - 1)
    results.append(check("beta-binomial: mean a/(a+b) = p", a / (a + b) - p))
    results.append(check("beta-binomial: corr 1/(a+b+1) = rho", 1 / (a + b + 1) - rho))

    def majority_right(nv, pv, rv):
        """Exact P(majority right) for odd nv voters, beta-binomial."""
        av = pv * (1 / rv - 1)
        bv = (1 - pv) * (1 / rv - 1)
        total = 0
        for kv in range(nv // 2 + 1, nv + 1):
            total += sp.binomial(nv, kv) * sp.beta(kv + av, nv - kv + bv) / sp.beta(av, bv)
        return sp.N(total, 30)

    pv, rv = sp.Rational(7, 10), sp.Rational(3, 10)
    theta = sp.Symbol("theta")
    av = pv * (1 / rv - 1)
    bv = (1 - pv) * (1 / rv - 1)
    ceiling = sp.N(sp.Integral(theta ** (av - 1) * (1 - theta) ** (bv - 1), (theta, sp.Rational(1, 2), 1))
                   / sp.beta(av, bv), 30)
    row = [(nv, majority_right(nv, pv, rv)) for nv in (1, 3, 5, 9, 15, 51)]
    print("     p=0.7, rho=0.3: P(majority right) by panel size:",
          ", ".join(f"n={nv}: {float(v):.4f}" for nv, v in row), f"; ceiling {float(ceiling):.4f}")
    increasing = all(row[i][1] <= row[i + 1][1] + sp.Float("1e-25") for i in range(len(row) - 1))
    below = all(v < ceiling for _, v in row)
    print(f"{'ok ' if increasing and below else 'FAIL'} majority accuracy rises toward P(theta>1/2) and stays below it")
    results.append(increasing and below)
    indep = sum(sp.binomial(51, kv) * pv**kv * (1 - pv) ** (51 - kv) for kv in range(26, 52))
    print(f"     independent voters at p=0.7, n=51: {float(indep):.6f} (the theorem's promise)")

    # 4. Extremizing. When each voter's log odds L_i are conditionally
    # independent evidence on a uniform prior, the posterior log odds are
    # sum L_i = n mean(L). The mean of the log odds is too timid by a factor n
    # (Baron et al. 2014, doi:10.1287/deca.2014.0293; Satopaa et al. 2014,
    # doi:10.1016/j.ijforecast.2013.09.009). With equicorrelated evidence of
    # equal strength the factor is n_eff, and nothing below checks it.
    L = sp.symbols("L0:3", real=True)
    prior = sp.Rational(1, 2)
    lik_yes = sp.prod([sp.exp(Li) / (1 + sp.exp(Li)) for Li in L])
    lik_no = sp.prod([1 / (1 + sp.exp(Li)) for Li in L])
    post_logodds = sp.log(lik_yes * prior / (lik_no * prior))
    results.append(check("posterior log odds = sum of log odds (independent evidence)",
                         sp.expand_log(sp.simplify(post_logodds), force=True) - sum(L)))

    # 5. Correlation from agreement, with two options. Voters i and j agree
    # when both are right or both are wrong. Their agreement is
    # A = P_bc + P_bw, with P_bw = 1 - p_i - p_j + P_bc, so
    # P_bc = (A - 1 + p_i + p_j) / 2 and
    #     rho_ij = (P_bc - p_i p_j) / sqrt(p_i (1-p_i) p_j (1-p_j)).
    pi_, pj, A, Pbc = sp.symbols("p_i p_j A P_bc", positive=True)
    Pbw = 1 - pi_ - pj + Pbc
    solved = sp.solve(sp.Eq(A, Pbc + Pbw), Pbc)[0]
    results.append(check("P(both right) = (A - 1 + p_i + p_j)/2", solved - (A - 1 + pi_ + pj) / 2))
    rho_ij = (solved - pi_ * pj) / sp.sqrt(pi_ * (1 - pi_) * pj * (1 - pj))
    # Independence means A = p_i p_j + (1-p_i)(1-p_j), which gives rho = 0.
    results.append(check("independent agreement gives rho = 0",
                         rho_ij.subs(A, pi_ * pj + (1 - pi_) * (1 - pj))))
    # Clones always agree: A = 1 with p_i = p_j gives rho = 1.
    clone = sp.refine(sp.simplify(rho_ij.subs({A: 1, pj: pi_})), sp.Q.lt(pi_, 1))
    results.append(check("clones (A = 1, p_i = p_j) give rho = 1", clone - 1))

    # 6. The gate on a short history. Two voters' correctness over m items
    # fills a 2x2 table: a both right, b only i, c only j, d neither. Their
    # Pearson correlation rho, as correlation.rs reads it, gives Pearson's
    # chi-square for the table as m rho^2. Its signed root, sqrt(m) rho, is
    # about N(0, 1) when the voters err apart. A pair counts toward the
    # discount only when sqrt(m) rho passes the one-sided five percent test,
    # z = 1.645.
    a, b, c, d = sp.symbols("a b c d", positive=True)
    m = a + b + c + d
    xbar, ybar = (a + b) / m, (a + c) / m
    cov = (a * (1 - xbar) * (1 - ybar) - b * (1 - xbar) * ybar
           - c * xbar * (1 - ybar) + d * xbar * ybar) / m
    rho_hat = cov / sp.sqrt(xbar * (1 - xbar) * ybar * (1 - ybar))
    cells = [(a, a + b, a + c), (b, a + b, b + d), (c, c + d, a + c), (d, c + d, b + d)]
    chi2 = sum((obs - row * col / m) ** 2 / (row * col / m) for obs, row, col in cells)
    results.append(check("m rho^2 is Pearson's chi-square of the 2x2 table", sp.factor(m * rho_hat**2 - chi2)))
    z95 = float(sp.sqrt(2) * sp.erfinv(sp.Rational(9, 10)))
    near = abs(z95 - 1.645) < 1e-3
    print(f"{'ok ' if near else 'FAIL'} the gate's 1.645 is the one-sided five percent point of N(0, 1), {z95:.6f}, to 1e-3")
    results.append(near)
    # Exact clones with both a hit and a miss among the shared items read
    # rho = 1, so the gate alone would pass them from three shared items,
    # where sqrt(m) > 1.645, but by default correlation's min_shared asks for
    # five. Independent voters pass the gate about one time in twenty.
    results.append(check("clones pass the gate from three shared items",
                         sp.Integer(int(sp.ceiling(sp.Float(1.645) ** 2))) - 3))

    print()
    if all(results):
        print(f"all {len(results)} checks hold")
        return 0
    print(f"{results.count(False)} of {len(results)} checks failed")
    return 1


if __name__ == "__main__":
    sys.exit(main())
