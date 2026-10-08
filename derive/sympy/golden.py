"""Exact fixed points for the Rust tests, computed in rationals with SymPy.

Run: python3 derive/sympy/golden.py   (rewrites derive/golden/fj.json)

Each case is raw trust rows, a self-weight rule and susceptibilities. The
row-stochastic W is built the way `influence_matrix_with` builds it, the
fixed point P = (I - L W)^-1 (I - L) is solved exactly (with the DeGroot
limit 1 pi^T for a closed class that listens fully), and P, the social
power c = (1/n) P^T 1 and the shares of the case's ballots are written as
decimal strings with 30 digits. src/lib.rs holds the solver to 1e-12.
"""

import json
import pathlib
import sys

import sympy as sp

R = sp.Rational


def matrix_of(agents, rows, self_trust, fallback):
    n = len(agents)
    idx = {a: i for i, a in enumerate(agents)}
    w = [[R(0)] * n for _ in range(n)]
    for f, t, x in rows:
        w[idx[f]][idx[t]] += R(x)
    inbound = []
    for j in range(n):
        named = [w[i][j] for i in range(n) if i != j and w[i][j] > 0]
        inbound.append(sum(named) / len(named) if named else None)
    for i in range(n):
        if all(x == 0 for x in w[i]):
            w[i] = [R(1)] * n
        elif w[i][i] == 0:
            if self_trust == "earned":
                w[i][i] = inbound[i] if inbound[i] is not None else R(fallback)
            else:
                w[i][i] = R(fallback)
        s = sum(w[i])
        w[i] = [x / s for x in w[i]]
    return sp.Matrix(w)


def fixed_point(W, s):
    n = W.shape[0]
    L = sp.diag(*s)
    A = L * W
    if all(x == 1 for x in s):
        # One closed class here by construction (every row has full support
        # in these cases): the DeGroot limit 1 pi^T.
        pi = sp.Matrix(sp.symbols(f"p0:{n}"))
        sol = sp.solve(list(pi.T * W - pi.T) + [sum(pi) - 1], list(pi))
        piv = sp.Matrix([sol[x] for x in pi])
        return sp.ones(n, 1) * piv.T
    return (sp.eye(n) - A).inv() * (sp.eye(n) - L)


CASES = [
    {
        "name": "learned rows, constant self-weight",
        "agents": ["a", "b", "c"],
        "rows": [["a", "b", "3/5"], ["a", "c", "1/5"], ["b", "a", "1"], ["b", "c", "1/5"],
                 ["c", "a", "1"], ["c", "b", "3/5"]],
        "self_trust": "constant", "fallback": "1/2",
        "s": ["1", "1", "1"],
        "ballots": {"a": "ship", "b": "ship", "c": "hold"},
    },
    {
        "name": "learned rows, earned self-trust: the weighted vote",
        "agents": ["a", "b", "c"],
        "rows": [["a", "b", "3/5"], ["a", "c", "1/5"], ["b", "a", "1"], ["b", "c", "1/5"],
                 ["c", "a", "1"], ["c", "b", "3/5"]],
        "self_trust": "earned", "fallback": "1/2",
        "s": ["1", "1", "1"],
        "ballots": {"a": "ship", "b": "hold", "c": "hold"},
    },
    {
        "name": "anchored personas over asymmetric rows",
        "agents": ["maintainer", "newcomer", "reliability", "reviewer"],
        "rows": [["maintainer", "reliability", "4/5"], ["newcomer", "maintainer", "1"],
                 ["newcomer", "reviewer", "1/2"], ["reliability", "reviewer", "3/10"],
                 ["reviewer", "maintainer", "7/10"], ["reviewer", "newcomer", "1/10"]],
        "self_trust": "constant", "fallback": "1/2",
        "s": ["1/5", "4/5", "2/5", "1/2"],
        "ballots": {"maintainer": "adapters", "newcomer": "status-quo",
                    "reliability": "adapters", "reviewer": "status-quo"},
    },
    {
        "name": "earned self-trust with one anchor: a count mixed with the weighted vote",
        "agents": ["a", "b", "c", "d"],
        "rows": [[f, t, x] for f in "abcd" for t, x in zip("abcd", ["1", "1/2", "1/4", "1/8"]) if f != t],
        "self_trust": "earned", "fallback": "1/2",
        "s": ["3/4", "3/4", "3/4", "3/4"],
        "ballots": {"a": "x", "b": "y", "c": "y", "d": "x"},
    },
]


def main():
    out = []
    for case in CASES:
        agents = case["agents"]
        W = matrix_of(agents, case["rows"], case["self_trust"], R(case["fallback"]))
        s = [R(x) for x in case["s"]]
        P = sp.simplify(fixed_point(W, s))
        n = len(agents)
        assert sp.simplify(P * sp.ones(n, 1) - sp.ones(n, 1)) == sp.zeros(n, 1), "P 1 = 1"
        c = (P.T * sp.ones(n, 1)) / n
        options = sorted(set(case["ballots"].values()))
        shares = {o: sum(c[i] for i, a in enumerate(agents) if case["ballots"][a] == o)
                  for o in options}
        dec = lambda x: str(sp.N(x, 30))
        out.append({
            "name": case["name"],
            "agents": agents,
            "rows": case["rows"],
            "self_trust": case["self_trust"],
            "fallback": case["fallback"],
            "s": case["s"],
            "ballots": case["ballots"],
            "p": [[dec(P[i, j]) for j in range(n)] for i in range(n)],
            "influence": [dec(c[i]) for i in range(n)],
            "shares": {o: dec(v) for o, v in shares.items()},
        })
        print(f"ok  {case['name']}: influence {[round(float(x), 4) for x in c]}")
    path = pathlib.Path(__file__).resolve().parent.parent / "golden" / "fj.json"
    path.parent.mkdir(exist_ok=True)
    path.write_text(json.dumps(out, indent=1) + "\n")
    print(f"wrote {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
