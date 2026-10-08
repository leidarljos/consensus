Why weigh at all
================

Counting is right when every voter is worth the same. A maintainer who has
read the code for years, a reviewer who was wrong twice, and a first-time
contributor each cast one ballot. A plurality reports them as three equal
opinions. DeGroot's model (doi:10.1080/01621459.1974.10480137) is the
standard answer and is one line: each voter replaces its opinion with the
weighted average of the voters it trusts, ``x(t+1) = W x(t)``. Where that
settles is the group's position, and it is the mean only when trust is
symmetric.

What a settle computes
======================

The step is linear in the ballots, so where it ends is too: ``x* = P x0``
for a matrix ``P`` whose rows each sum to one. The shares are the mean row,
so a settle is a weighted vote. Voter i's ballot counts with weight
``c_i = (1/n) sum_k P_ki``, its social power (Friedkin,
doi:10.1086/229694; Proskurnikov and Tempo,
doi:10.1016/j.arcontrol.2017.03.002). Every outcome prints it as
``influence``, beside ``effective_voters``, the inverse Herfindahl index
``1 / sum c_i^2``: the number of equal voices the settle is worth. A crowd
is wise only while no voice carries a share of the result that stays put
as the crowd grows (Golub and Jackson, doi:10.1257/mic.2.1.112), and
DeGroot's averaging counts the well connected again each round
(DeMarzo, Vayanos and Zwiebel, doi:10.1162/00335530360698469). A settle
with ``effective_voters`` near one is one voter's opinion, however many
ballots it read.

``--engine exact`` reads ``P`` off directly: the linear system
``(I - L W) P = I - L`` where some voter is anchored, the stationary
distribution of a closed group that listens fully. The iteration remains
the default and lands on the same point.

Whose own voice counts
======================

A trust row says how much a voter listens to each of the others. What it
weighs its own ballot is the diagonal, and a row that does not say gets a
fill. The tracker fills one constant for every voter. Under the rows that
``learn`` and ``calibrate`` write, every voter weighs voter j alike, as ``w_j``,
and with a constant self-weight ``sw`` the settle then weighs voter j by
``w_j (S + sw - w_j)``, ``S`` the sum of the weights: not by ``w_j``. The best
voter's lead shrinks, because its own row sends the same constant home as
the weakest voter's does.

``--self-trust earned``, the default since 0.7.0, fills the diagonal with
what the others give the voter: the mean of the weights the rows that name
it put on it. Every row is then ``w / S``, the matrix is idempotent, one
round leaves every voter on the weighted vote ``sum_j w_j x0_j / S``, and
with log-odds rows that is the Nitzan and Paroush optimum. With every
voter anchored at ``s`` the settle is the mixture ``(1 - s) count + s
weighted vote``, so the anchor reads as how far the group trusts its record
over a show of hands. The empirical side is the same: people who revise
least toward the group are the accurate ones, and weighing them more
improves the crowd (Becker, Brackbill and Centola,
doi:10.1073/pnas.1615978114; Madirolas and de Polavieja,
doi:10.1371/journal.pcbi.1004594).

Measured where the truth is known (``examples/synthetic_voters.rs``, twenty
seeds of four hundred two-way questions), the difference is nothing when
accuracies are spread evenly: nine voters drawn in [0.35, 0.95] score
0.934 and 0.933 for calibrate, fifteen 0.974 and 0.975. It shows where a
weak crowd can outvote the one voter worth hearing. With one voter at
0.92 among eight in [0.52, 0.62], calibrate goes from 0.887 to 0.907 and
the running record from 0.901 to 0.913, against the 0.919 the true
weights reach. ``--self-trust constant`` keeps the old fill.

When it settles
===============

The iteration converges to agreement exactly when the trust graph has one
closed group every voter reaches, and that group is aperiodic (Berger,
doi:10.1080/01621459.1981.10477662). Two teams that cite only each other
never converge, and that is a fact about the team worth reporting rather
than a number worth averaging. The tracker's own ``consensus`` verb reports
the split; this crate reports ``settled: false`` when the budget runs out,
and the exact engine names the cycle.

``settled`` means within ``--tol`` of the fixed point, not that the last step
was small. With every voter anchored, ``q`` the largest susceptibility, the
step is a ``q``-contraction in the largest-entry norm, so one step from x
the fixed point is no farther than ``q / (1 - q)`` times the step's length
(Banach's estimate). The iteration stops when that bound falls under the
tolerance, and ``residual`` is the bound. A small step alone said little:
at ``q = 0.99`` the opinions could sit a hundred times the step away. Where
some voter listens fully there is no such contraction, and ``residual`` is
the distance to the closed-form point. Binary64 adds at most
``gamma(n + 2) / (1 - q)`` to the bound, Higham's error of an ``n``-term dot
product; at ``q = 0.99`` that floor is 1.1e-12 for a hundred voters, so the
default tolerance of 1e-9 sits far above it. The bound needs more than the
default two hundred rounds once ``q`` passes 0.89; a panel that has arrived
sooner than its bound admits is then settled on the measured distance.

``tie`` says the leading two shares are closer than twice the residual plus
rounding: the settle cannot order them, and naming the first would be a
coin the reader did not see tossed.

Anchoring
=========

Friedkin and Johnsen (doi:10.1080/0022250X.1990.9990069) keep each voter
partly anchored to its own starting opinion. The step is a contraction for
any anchor, so it always settles, and what settles is a profile of
persistent disagreement rather than one shared position. That is the
setting to use when reviewers are not expected to abandon their reading.

Bounded confidence
==================

Hegselmann and Krause, and Deffuant and colleagues
(doi:10.1142/S0219525900000078), let each voter listen only to voters
whose opinion lies within a bound of its own. Two blocs further apart than
the bound never meet: the model produces clusters where the trust graph
alone would produce one position. ``--epsilon`` runs that step, and
``--epsilon-of`` gives each voter its own bound, which is the shape a persona
with a narrow audience takes. Castellano, Fortunato and Loreto review the
family (doi:10.1103/RevModPhys.81.591).

Reading a settle
================

Two numbers come with every outcome. Polarization is how far the final
opinions sit from their mean, summed over voters; disagreement is how far
neighbours sit from each other, weighted by the trust between them, summed
over edges. Musco, Musco and Tsourakakis (doi:10.1145/3178876.3186103)
show the two move against each other under the Friedkin-Johnsen step, so a
network can be tuned to lower their sum. A settle that reports high
polarization and low disagreement has agreed within blocs that do not hear
each other; the shares alone would not show it.

Panels of agents
================

Running several model calls and settling their answers is now a product
feature. Self-consistency samples one model many times and takes the
majority (Wang et al., doi:10.48550/arXiv.2203.11171). Multi-agent debate
lets several instances read each other's answers over rounds before a
final vote (Du et al., doi:10.48550/arXiv.2305.14325; Liang et al.,
doi:10.48550/arXiv.2305.19118), and a jury of debating evaluators grades
text the same way (ChatEval, doi:10.48550/arXiv.2308.07201). Mixture of
agents feeds the answers of several models to an aggregator model that
writes the final one (Wang et al., doi:10.48550/arXiv.2406.04692). One
model can also play several personas and have them collaborate (Wang et
al., doi:10.48550/arXiv.2307.05300), and the roster can be chosen per task
(Liu et al., doi:10.48550/arXiv.2310.02170). Debate between more
persuasive models makes a weaker judge more truthful (Khan et al.,
doi:10.48550/arXiv.2402.06782). A commercial system runs several agents in
parallel on "multiple hypotheses at once" and settles before answering
(the Grok 4 announcement, https://x.ai/news/grok-4); reports describe a
captain agent deciding when the others do not agree, which this crate has
not verified against a primary source.

Two things these share, and this crate does differently. First, the
aggregation is a count or an aggregator model's judgement, with every
voice weighed the same; Chen et al. (doi:10.48550/arXiv.2403.02419) show
that adding calls to such a count is not even monotone in accuracy, since
it helps on easy items and hurts on hard ones. Here the aggregation is a
weighted settle whose weights are memory: rows a person set, rows
``learn`` moved when an outcome refuted a voter, rows ``calibrate`` wrote
from the voters' history, each scoped to the topics they were earned on.
Second, the roles are prompts that vanish with the session; here a persona
is an atom in the pack with an anchor the settle honours, and a captain
who decides when the panel splits is one persona with an anchor of zero,
not a special case. The settle also says how the panel split, in
polarization and disagreement, where a count says only who won.

When the majority is wrong
==========================

A settle over ballots cannot see a majority that is wrong, only a majority.
Prelec, Seung and McCoy (doi:10.1038/nature21054) ask each voter one more
thing: what share of the others will pick each option. A minority that
knows better predicts the majority and votes against it, so the option
whose actual share most exceeds its predicted share is the informed one;
on questions where most people are confidently wrong, that rule recovers
the truth where a majority and a confidence-weighted vote both fail. It is
the second verb a panel runs when the question is hard: ``surprising`` reads
the ballots and the forecasts and names the answer, or says that fewer than
two voters forecast and it has nothing to add. Prediction polls of this
shape score about as well as prediction markets on the same questions
(Atanasov et al., doi:10.1287/mnsc.2015.2374).

Standing from the rows
======================

Pairwise rows say who listens to whom; they do not say who stands high in
the group. EigenTrust (Kamvar, Schlosser and Garcia-Molina,
doi:10.1145/775152.775242) reads that off the same rows: normalise them,
take the principal eigenvector, and pull toward a uniform pre-trust so
nobody falls to zero and the iteration settles. A voter weighed by voters
who are themselves weighed stands high; a row from a voter nobody weighs
counts for little. ``reputation`` prints the standing; the seat shows it
beside a settle so a reader sees not only where the group landed but
whose word carried it.

Where the rows come from
========================

In the tracker, rows are configuration. In the seat, rows are memory: the
pack's ``trust`` atoms, dated and supersedable, moved by ``ljos learn`` when
an outcome shows who was right. This crate takes rows from anywhere as
JSON and does not care which.

Outcomes are rare; most issues close without anyone saying which option
was right. Dawid and Skene (doi:10.2307/2346806) estimate each voter's
accuracy without a truth, from how often it agrees with the answer the
other voters make likely, by expectation maximisation. ``reliability`` runs
that over a project's settled issues, and ``ljos calibrate`` turns the
accuracies into rows, so weights move from the tracker's own history. The
row a voter gets is its accuracy, which is the weight a linear opinion pool
gives a source believed that reliable (Genest and Zidek,
doi:10.1214/ss/1177013825). Acemoglu, Como, Fagnani and Ozdaglar
(doi:10.1287/moor.1120.0570) show what a stubborn voter does to such a
pool: with an anchor near one it never moves and pulls the rest, which is
why a persona's anchor is a parameter here and not a default.

The rules were measured where the truth is known
(``examples/synthetic_voters.rs``): voters with accuracies drawn uniformly
between 0.35 and 0.95, so some are worse than chance, answer two-way
questions, and the group decides under each rule; twenty seeds, four
hundred questions each. Nine voters: one voter one vote 0.820; the
accuracies as linear weights 0.873; the accuracies as log odds
(Nitzan and Paroush, doi:10.2307/2526438) 0.934 against 0.939 for the
same weights from the true accuracies, the ceiling; Hedge from equal
rows 0.831, because after enough misses every row sits on the floor and
the settle is a count again; Hedge with a fixed share of recovery
(Herbster and Warmuth, doi:10.1023/A:1007424614876) 0.877; and the
online form of calibration, each voter's smoothed record of hits and
misses so far as log odds before every question, 0.929. Fifteen voters:
0.877, 0.936, 0.974, 0.976, 0.891, 0.937, 0.972. Log-odds weights reach
within half a point of the ceiling whether estimated in one batch or
kept as a running record; multiplicative shrinking does not, even with
recovery. ``ljos calibrate`` writes log odds for that reason, and ``ljos
learn`` keeps the record.

Voices that share a cause
=========================

The jury theorem's promise, that a majority of better-than-chance voters is
right more often the larger it grows, rests on independent errors
(Grofman, Owen and Feld, doi:10.1007/BF00125672). Votes driven by a common
cause are correlated, and correlation caps what more voters can add
(Ladha, doi:10.2307/2111584; Kaniovski, doi:10.1007/s11238-008-9120-4;
Dietrich and Spiekermann, doi:10.1093/mind/fzt074). Personas answered by
one model share that model: across more than 350 language models, two that
both err agree on the wrong answer about 60% of the time, and more accurate
models err more alike (Kim, Garg, Peng and Garg,
doi:10.48550/arXiv.2506.07962). Five personas of one model are not five
votes.

``derive/sympy/jury.py`` does the arithmetic. ``n`` voters correlated ``rho``
are worth ``n / (1 + (n - 1) rho)`` independent ones, at most ``1 / rho``
however many sit. Modelling the common cause as a latent accuracy, the
majority's chance saturates: at a mean accuracy of 0.7 and ``rho = 0.3`` a
panel of 51 is right 0.770 of the time against a ceiling of 0.773, where
independent voters would reach 0.9986. The weights that make the most of a
cluster divide each member by ``1 + (k - 1) rho``, so a cluster of ``k``
counts as ``k / (1 + (k - 1) rho)`` voices and, as ``rho`` goes to one, as
one.

``correlation`` reads ``rho`` off a project's history: each voter's
correctness on each item, against the outcome where one was named and the
Dawid-Skene answer otherwise, and the correlation of those indicators per
pair. It prints the discount each voter keeps, ``1 / (1 + sum_k rho_ik)``
over the pairs it counts, for ``settle --discount-of``, the panel's
``effective_voters`` under equal weights, and its ``independent_voters`` once
discounted. Measured on five clones of one judge right 0.70 of the time
beside four independent voters in [0.60, 0.75]
(``examples/correlated_voters.rs``, the discount re-read every 25 questions
against the named outcomes), a count scores 0.701, log-odds weights 0.710,
the same weights with the discount 0.797, and the true weights with the
clones merged 0.805.

The reading needs those outcomes. Against the Dawid-Skene answer instead
(the example's ``infer`` argument), the clone bloc is most of the answer it
is read against, so its members look right on every item, their
correctness never varies, and their correlation reads zero: the reading
holds 8.51 independent voices where there are five, and the discount
leaves the group at 0.706. It does no harm on a panel with no clones
(0.831 against 0.834), but it buys nothing without named outcomes. The
seat records one each time ``ljos learn`` names the option an issue closed
on, and applies the discount once five exist.

On a short history
------------------

A seat decides a handful of issues, then tens, so both readings it learns
from are short. Over ``m`` shared items the correlation of two voters who
err apart reads about ``N(0, 1/m)``, and counting every positive reading
discounts independent voters by noise. ``correlation`` therefore counts a
pair only when ``sqrt(m) rho`` passes the one-sided test of independence at
five percent, 1.645 (``--gate``): ``m rho^2`` is Pearson's chi-square for the
pair's two-by-two table of right and wrong, which ``derive/sympy/jury.py``
checks. Exact clones read one and pass from three shared items; voters who
err apart pass about one time in twenty.

A voter's record is as short. Log odds of a record of five (Nitzan and
Paroush's weights, plugged in) drop a good voter who started unlucky to the
floor, and on a panel of similar voters they lose to a plain count. The
cure is old: shrink each voter's accuracy toward the panel's pooled
accuracy, with the prior's strength read from how much more the records
differ than sampling alone would make them, by empirical Bayes (Efron and
Morris, doi:10.1080/01621459.1975.10479864). The sampling part is the
pooled variance with ``N / (N - 1)``, unbiased; on a panel's first outcome
the spread between voters cannot be told from noise, and every voter
weighs the same. A short record weighs the voters alike, and a long one
keeps the differences it has shown. ``examples/correlation_history.rs``
measures the decision after ``h`` named outcomes, 4000 seeds a cell:

================================== ===== ===== ======== ======================== ======================
Panel                              ``h`` Count Log odds Log odds, gated discount Shrunk, gated discount
================================== ===== ===== ======== ======================== ======================
5 clones, 4 voters in [0.60, 0.75] 3     0.698 0.730    0.730                    0.702
\                                  5     0.700 0.719    0.736                    0.780
\                                  100   0.700 0.708    0.805                    0.813
7 voters in [0.60, 0.75]           1     0.837 0.812    0.812                    0.837
\                                  5     0.843 0.780    0.773                    0.826
\                                  100   0.836 0.824    0.821                    0.834
7 voters in [0.55, 0.90]           5     0.904 0.884    0.876                    0.896
\                                  30    0.888 0.909    0.906                    0.902
\                                  100   0.900 0.920    0.919                    0.918
3 clones, 4 voters in [0.55, 0.90] 5     0.780 0.789    0.808                    0.830
\                                  100   0.774 0.830    0.876                    0.879
================================== ===== ===== ======== ======================== ======================

Ungated, the discount costs the panel of seven similar voters three points
at five outcomes (0.750 against 0.780); gated, less than one (0.773).
Shrinking and the gated discount together gain on plug-in log odds in
every panel from five outcomes on, by up to ten points where clones sit,
except on a long record of widely differing voters, where they trail by
under a point. Before five outcomes the discount reads no pair, and where
clones sit they trail by up to three points, because plug-in log odds'
unlucky flooring happens to break the bloc. Where a count is best they stay
within two points of it, and on the first outcome they are the count.
``ljos learn`` shrinks for that reason.

Independence before influence
=============================

A settle is social influence run to its end, and influence can destroy
the information a crowd holds. Ballots seen before ballots are cast become
cascades: each voter rationally follows the ones before it and its private
evidence never reaches the count (Bikhchandani, Hirshleifer and Welch,
doi:10.1086/261849). Exposure to others' estimates narrows a crowd
without making it more accurate (Lorenz, Rauhut, Schweitzer and Helbing,
doi:10.1073/pnas.1008636108), and people give a wrong answer to agree
with a room (Asch, doi:10.1037/h0093718). The Delphi method collects
judgments apart for that reason (Dalkey and Helmer,
doi:10.1287/mnsc.9.3.458). Panels of language models repeat it: most of
what multi-agent debate gains, majority voting over the first answers
already had, and debate alone leaves expected correctness where it was
(Choi, Zhu and Li, doi:10.48550/arXiv.2508.17536).

So the ballots a settle reads should be cast before any voter saw another's.
The seat's panel hands each persona a brief that carries no ballot and
records each vote before it prints the tally, and the settle does the
listening afterwards, where it can be weighed and reported, rather than in
the voters' heads, where it cannot.

Derivations, proofs and certificates
====================================

The claims above are checked, not only cited. ``derive/`` holds three kinds
of evidence, each runnable:

================== ====================================================== =====================================================================================================================================================================================================================
Tool               Run                                                    What it shows
================== ====================================================== =====================================================================================================================================================================================================================
SymPy              ``python3 derive/sympy/fj.py``                         ``P`` is row-stochastic; the social power; the ``w_j (S + sw - w_j)`` weights a constant self-weight gives; earned self-trust as the weighted vote; the mixture under one anchor; the second eigenvalue
SymPy              ``python3 derive/sympy/jury.py``                       ``n_eff``; the cluster discount; the beta-binomial ceiling; extremizing; ``rho`` from agreement; ``m rho^2`` as the two-by-two chi-square behind the gate
SymPy              ``python3 derive/sympy/golden.py``                     four settles solved in rationals, which the Rust tests match to 1e-12
Lean 4 and Mathlib ``cd derive/lean && lake exe cache get && lake build`` the step is a contraction with one fixed point the iteration reaches; Banach's stopping bound; rows stay distributions; Nitzan and Paroush's theorem; idempotent earned rows; the stationary weights of constant ones
Sollya             ``sollya derive/sollya/rounding.sollya``               in interval arithmetic: the rounding floor of the bound, the rounds it needs, the tie allowance, the log-odds range of a record
================== ====================================================== =====================================================================================================================================================================================================================

The Lean theorems rest on the three standard axioms and nothing else.

Seldon
======

Seldon is an opinion-dynamics engine that integrates the same models as
ordinary differential equations. This crate can write its inputs and read its output so the discrete
step can be checked against a second implementation. It does not link
Seldon; it runs the binary when present.
