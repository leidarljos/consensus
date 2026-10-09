Where the settle's guarantees come from: the closed form the iteration
solves for, the round count that follows from it, the floating-point
budget the stopping rule respects, the proof assistant the statements
mirror, and the social science behind the models. The :doc:`explanation`
says what each model means; this page shows why the code may claim what it
claims.

The Friedkin-Johnsen step as a contraction
==========================================

One round is ``x(t+1) = (I - S) x0 + S W x(t)``, with ``W`` row-stochastic
and ``S`` the diagonal matrix of susceptibilities. Subtracting two
trajectories cancels the anchor term, and a row-stochastic matrix never
stretches the sup norm, so successive differences shrink by ``max s_i``
per round. The fixed point satisfies ``(I - S W) x* = (I - S) x0``, hence
the closed form ``x* = (I - S W)^{-1} (I - S) x0``, which is invertible
exactly when every voter keeps some anchor. The symbolic check is three
lines: expand ``(I - S W)^{-1}`` as the Neumann series, multiply by
``(I - S) x0``, and recover the iteration's limit term by term. In SymPy:

.. code:: python

   import sympy as sp
   n = 3
   S = sp.diag(*sp.symbols('s0:3'))
   W = sp.MatrixSymbol('W', n, n)
   I = sp.Identity(n)
   x0 = sp.MatrixSymbol('x0', n, 1)
   xstar = (I - S*W).as_explicit().inv() * (I - S).as_explicit() * x0.as_explicit()
   # series: x(t+1) - x(t) = (S W)^t (x(1) - x(0)), so ||.||_inf <= max(s)^t.

The code carries the two consequences as functions, not comments:
``fj_contraction_bound`` returns ``max s_i`` over the voters, and
``fj_rounds_to_tol`` turns it into a round count: the first step moves no
row by more than the bound (both ``W x(0)`` and ``x(0)`` are rows of
distributions, so their sup distance is at most one), and successive
differences shrink by the bound, so ``err(t) <= bound^t`` and
``t >= ln(tol)/ln(bound)`` suffices, plus the first step. A test holds an
anchored settle to the predicted count. When the bound is not a contraction
(``s_i = 1`` everywhere, pure DeGroot), the function returns nothing: there
is no round count to guarantee, convergence is asymptotic under Berger's
closed-group conditions (doi:10.1080/01621459.1981.10477662), and the
iteration budget (``max_iter``) decides. That ``None`` is the honest boundary
between what is proved and what is budgeted.

The floating-point budget
=========================

A contraction in the reals still stops in floating point, and the stopping
rule must say what it stopped on. The iteration reports ``settled: false``
when the budget runs out rather than averaging a split, and the energy
engine (``fj-energy``) reports the largest component of the energy's gradient
at the point returned as ``residual``: zero when an engine does not measure
it. The settled test there is the tolerance asked, scaled by the square root
of the number of voters so a wide panel is not held to a tighter bar per
voter than a small one, or the floor floating point sets, whichever is
larger: once the decrease a step could buy, of the order of the squared
gradient, is below the energy's own rounding, no line search can accept a
step, and the point is as settled as the arithmetic allows. That floor is the
part of the routine a Sollya-style error analysis justifies: bound the
rounding of the energy and its gradient, and the stopping test falls out. The
interval half is done: ``scripts/verify_fj.py`` replays an anchored settle in
outward-rounded intervals (``mpmath.iv``, the same rounding discipline Sollya
certifies) and shows the float stop and the closed form both lie in an
enclosure 5e-16 wide, so stopping at ``tol`` 1e-9 is far above the rounding
floor. What remains open is the certificate for the compiled Rust evaluation
order rather than the script's: a Sollya proof over the emitted
floating-point operations. Until then the claim is the enclosure, not the
certificate, and the page says so.

The Lean correspondence
=======================

The statements above mirror lemmas available in Lean's Mathlib, and new
proof work should target them rather than restating them:

- ``W`` is row-stochastic; rows of distributions stay rows of
  distributions: ``Matrix.IsStochastic`` and the simplex lemmas around it.
- The sup norm never grows under a stochastic row: operator-norm lemmas for
  stochastic matrices.
- The anchored map is a contraction with factor ``max s_i`` below one: the
  Banach fixed-point theorem (``ContractingWith.fixedPoint``).
- The Neumann series gives the closed form: geometric-series lemmas for
  normed rings.
- EigenTrust standing is the principal eigenvector pulled toward pre-trust:
  Perron-Frobenius for nonnegative matrices.

Nothing here is a verified proof yet: the table above is a map of where the
proofs would land, so a future formalisation starts from named lemmas rather
than from prose. The code's part is to keep the statements proof-shaped: pure
functions over explicit matrices, bounds returned as values, and the unproved
case (``None``) distinguished from the proved one.

What the social science adds
============================

The models are mathematics; when to use each is social science:

- Simple contagion spreads on contact, complex contagion needs several
  reinforcing neighbours (Centola and Macy, doi:10.1086/521848; Centola,
  doi:10.1126/science.1191287). A panel that must be convinced rather than
  informed wants the anchored step with low susceptibility, not a count: one
  exposure should not move a voter.
- Thresholds differ per person (Granovetter, doi:10.1086/225469). That is
  what ``--epsilon-of`` and per-voter anchors are: a persona with a narrow
  audience listens only to those near it, and the bound is a parameter of the
  voter, not of the group.
- A minority can move a group without authority when it is consistent
  (Moscovici, Lage and Naffrechoux, doi:10.2307/2786272). The
  surprisingly-popular rule (``surprising``) is the ballot-box form: a
  minority that knows better predicts the majority and votes against it, and
  the option whose actual share most exceeds its predicted share is the
  informed one (Prelec, Seung and McCoy, doi:10.1038/nature21054).
- Disagreement is structural, not residual. The settle reports polarization
  and disagreement with every outcome (Musco, Musco and Tsourakakis,
  doi:10.1145/3178876.3186103): a settle that reports high polarization and
  low disagreement has agreed within blocs that do not hear each other, and
  the shares alone would not show it.

Reproducing the numbers
=======================

The calibration table on the explanation page regenerates from
``examples/synthetic_voters.rs``: voters with accuracies drawn uniformly
between 0.35 and 0.95 answer two-way questions under each rule, twenty
seeds, four hundred questions each. The contraction test runs with the unit
tests: ``cargo test contraction`` holds the anchored settle inside the round
count the bound predicts. The full derivation is checked outside Rust too:
``python3 scripts/verify_fj.py`` (SymPy; ``pip install sympy``) verifies the
exact rational fixed point, the Neumann-series limit, the per-round
contraction, the round-count prediction, and the shipped ``ljos-consensus``
binary's agreement with the closed form over eight random panels, and
``.github/workflows/test.yml`` runs it with ``cargo test`` on every push.
