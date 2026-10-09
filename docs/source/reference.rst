Command line
============

``ljos-consensus settle [FLAGS]``

=================================== ==============================================================================================================================================================================================================================
Flag                                Meaning
=================================== ==============================================================================================================================================================================================================================
``--issue ID``                      ballots from ``vissue vote ID --json``
``--ballots JSON``                  ``[{agent, choice}]``, or an object with ``ballots``, ``votes`` or ``agents[].voted``
``--trust JSON``                    ``[[from, to, weight], ...]`` or ``[{from, to, weight}, ...]``
``--self-trust MODE``               how a voter weighs its own ballot when its row does not say: ``earned`` (the default) as the others weigh it, or ``constant`` at ``--self-weight`` for every voter
``--self-weight W``                 the self-weight under ``constant``, and the fallback under ``earned`` for a voter nobody names; default 0.5
``--discount-of JSON``              ``\{agent: d\}``, the share of its inbound weight each voter keeps; ``correlation`` prints it
``--susceptibility S``              1 is DeGroot; below 1 anchors each voter to its ballot; default 1
``--susceptibility-of JSON``        ``\{agent: s\}``, a persona's own anchor per voter
``--epsilon E [--epsilon-of JSON]`` bounded confidence instead of the trust graph: each voter averages only voters within L1 distance ``E`` of its own opinion
``--max-iter N``, ``--tol T``       the fixed-point budget; defaults 200 and 1e-9
``--engine NAME``                   ``iterate`` (the default) steps to the fixed point and stops on a bound; ``exact`` reads it off in closed form; ``energy`` minimises the Friedkin-Johnsen energy under a constant ``--self-weight``, without ``--discount-of``
``--seldon [--out DIR]``            write Seldon inputs, run ``seldon``, read the result
=================================== ==============================================================================================================================================================================================================================

Output:

==================== ============================================================================================
Field                Meaning
==================== ============================================================================================
``options``          the options, sorted
``shares``           each option's share, in that order
``rounds``           the steps taken
``settled``          within ``--tol`` of the fixed point
``residual``         the bound or distance that says so
``engine``           ``degroot-fj``, ``fj-exact``, ``fj-energy``, ``bounded-confidence``, ``seldon`` or ``empty``
``polarization``     the sum over voters of the squared distance from the mean final opinion
``disagreement``     the sum over trust edges of weight times the squared distance between the two ends
``agents``           the voters
``influence``        each voter's social power: the weight its ballot carries in the shares, summing to one
``effective_voters`` ``1 / sum influence^2``
``margin``           the leading share less the next
``tie``              the margin is within twice the residual and rounding
==================== ============================================================================================

Polarization and disagreement follow Musco, Musco and Tsourakakis
(doi:10.1145/3178876.3186103). Only the iterate and exact engines fill the
last five fields.

``ljos-consensus surprising (--issue ID | --ballots JSON) --predictions JSON``

The surprisingly popular answer (Prelec, Seung and McCoy,
doi:10.1038/nature21054). ``--predictions`` is an array of ``{agent, expect}``
where ``expect`` is an option, or an object of option to the share the voter
expects the others to give it. Output: ``options``, ``actual`` and ``predicted``
shares, ``surprise`` (actual minus predicted), ``answer`` (the largest
surprise; absent below two predictors), ``predictors``.

``ljos-consensus reputation --trust JSON [--agents a,b,c] [--alpha A]``

EigenTrust (Kamvar, Schlosser and Garcia-Molina,
doi:10.1145/775152.775242): a global standing per voter from the pairwise
rows, the principal eigenvector of the row-normalised trust matrix pulled
toward a uniform pre-trust by ``alpha`` (default 0.15). Output: ``standing``
as an object of voter to a share of one.

``ljos-consensus reliability (--items JSON | --project P) [--rounds N]``

Dawid and Skene's estimate (doi:10.2307/2346806) of each voter's accuracy
from many items with no known truth: expectation maximisation over the
items' hidden answers and the voters' accuracies, twenty rounds by default.
``--items`` is a JSON array of ballot arrays; ``--project`` reads every issue
of a tracker project that holds two or more ballots. Output: ``items``,
``rounds``, and ``accuracy`` as an object of voter to a value in (0, 1),
Laplace smoothed so no voter reaches a certainty.

``ljos-consensus correlation (--items JSON [--truths JSON] | --project P) [--min-shared N] [--gate Z] [--rounds N]``

How much the voters share their mistakes. A voter's correctness on each
item is read against the item's named outcome, or else the Dawid-Skene
answer. ``--truths`` names the outcomes, one string or null per item. ``rho``
is the Pearson correlation of those indicators for each pair, over the
items both voted on; it is zero below ``--min-shared``, which defaults to 5.
A pair counts only when ``sqrt(shared) rho`` passes ``--gate``, the one-sided
test of independence. The gate defaults to 1.645, five percent, and 0
counts every positive reading.

====================== =========================================================================
Field                  Meaning
====================== =========================================================================
``agents``             the voters, in the order of the matrices
``rho``, ``shared``    the correlations, and the items each pair shares
``gate``               the test a pair passed to count
``discount``           voter to ``1 / (1 + sum_{j !`` i} rho\ :sub:`ij`)= over the counted pairs
``effective_voters``   ``n^2 / sum_ij rho_ij`` over the counted pairs: what equal weights get
``independent_voters`` the sum of the discounts
``items``, ``named``   the items read, and how many had a named outcome
====================== =========================================================================

The step
========

``x(t+1) = (1 - s) x(0) + s W x(t)``, with ``W`` the row-stochastic trust
matrix and ``s`` the susceptibility. A voter's opinion is a distribution
over the options, one-hot at the start. ``--self-trust`` fills a missing
self weight. A voter with no row of its own listens to everyone, itself
included, each in proportion to its discount, so equally when there is
none. A voter's inbound weight is scaled by its discount before the rows
are normalised. Shares are the column sums of the fixed point, normalised.

The fixed point is ``x* = P x(0)``. ``P`` is ``(I - L W)^{-1} (I - L)``, with
``L`` the diagonal of susceptibilities, wherever every closed group of the
trust graph holds an anchored voter; a closed group that listens fully
settles on its stationary distribution, and the voters outside it solve
against that. The iteration stops when ``q / (1 - q)`` times its last step,
plus the binary64 floor ``gamma(n + 2) / (1 - q)``, is under ``--tol``, with
``q`` the largest susceptibility. A panel where some voter listens fully
stops on its distance to ``x*``, or on its last step when ``x*`` cannot be
read off.

Library
=======

The crate root exports the types ``Ballot``, ``Outcome``, ``Opts``, ``SelfTrust``,
``Prediction`` and ``Surprise``, and the helpers ``roster``, ``influence_matrix``
and ``influence_matrix_with``. Its settles are ``settle``, ``settle_anchored``,
``settle_with``, ``settle_exact``, ``settle_bounded``, ``settle_energy`` and
``settle_seldon``. Its readings are ``dawid_skene``, ``surprisingly_popular``
and ``eigentrust``; its parsers are ``ballots_from_json``, ``trust_from_json``,
``anchors_from_json`` and ``predictions_from_json``. ``exact`` holds
``fixed_point``, ``social_power``, ``effective_voters`` and ``Unsettled``, the
reason a fixed point could not be read off. ``correlation`` holds the
reading above and ``INDEPENDENCE_Z``. The Seldon module writes and reads the
engine's files and links nothing under a copyleft licence.
