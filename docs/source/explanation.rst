===========
Explanation
===========



Why weigh at all
----------------

Counting is right when every voter is worth the same. A maintainer who has
read the code for years, a reviewer who was wrong twice, and a first-time
contributor each cast one ballot. A plurality reports them as three equal
opinions. DeGroot's model (https://doi.org/10.1080/01621459.1974.10480137) is the
standard answer and is one line: each voter replaces its opinion with the
weighted average of the voters it trusts, ``x(t+1) = W x(t)``. Where that
settles is the group's position, and it is the mean only when trust is
symmetric.

When it settles
---------------

The iteration converges to agreement exactly when the trust graph has one
closed group every voter reaches, and that group is aperiodic (Berger,
https://doi.org/10.1080/01621459.1981.10477662). Two teams that cite only each other
never converge, and that is a fact about the team worth reporting rather
than a number worth averaging. The tracker's own ``consensus`` verb reports
the split; this crate reports ``settled: false`` when the budget runs out.

Anchoring
---------

Friedkin and Johnsen (https://doi.org/10.1080/0022250X.1990.9990069) keep each voter
partly anchored to its own starting opinion. The step is a contraction for
any anchor, so it always settles, and what settles is a profile of
persistent disagreement rather than one shared position. That is the
setting to use when reviewers are not expected to abandon their reading.

Where the rows come from
------------------------

In the tracker, rows are configuration. In the seat, rows are memory: the
pack's ``trust`` atoms, dated and supersedable, moved by ``ljos learn`` when
an outcome shows who was right. This crate takes rows from anywhere as
JSON and does not care which.

Seldon
------

Seldon is an opinion-dynamics engine that integrates the same models as
ordinary differential equations. This crate can write its inputs and read its output so the discrete
step can be checked against a second implementation. It does not link
Seldon; it runs the binary when present.
