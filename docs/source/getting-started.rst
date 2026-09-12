===============
Getting started
===============


Three voters, two options, and a trust graph that changes the answer.

1. Settle with equal trust
--------------------------

.. code:: text

    $ ljos-consensus settle --ballots '[
        {"agent":"alice","choice":"ship"},
        {"agent":"bob","choice":"ship"},
        {"agent":"carol","choice":"hold"}]'
    { "options": ["hold","ship"], "shares": [0.333, 0.667], "rounds": 1, "settled": true, "engine": "degroot-fj" }

With no rows, every voter's row is itself, so the settle is the tally as a
fraction.

2. Add trust
------------

.. code:: text

    $ ljos-consensus settle --ballots '[...]' --trust '[
        ["alice","carol",1.0], ["bob","carol",1.0], ["carol","carol",4.0], ["carol","alice",1.0]]'
    { ... "shares": [0.6, 0.4] ... }

Alice and bob each listen to carol as much as to themselves; carol weighs
herself four to one over alice. The group moves toward carol's ``hold``.

3. Anchor the voters
--------------------

.. code:: console

    $ ljos-consensus settle --ballots '[...]' --trust '[...]' --susceptibility 0.5

Below one, each voter stays partly anchored to the ballot they cast. The
run always settles, and it settles on voters who still disagree; the shares
report where the group's weight ended up.

4. Read ballots from a tracker
------------------------------

.. code:: console

    $ ljos-consensus settle --issue proj-1a2b

Runs ``vissue vote proj-1a2b --json`` and settles those ballots. The seat's
``ljos consensus`` adds the pack's trust rows to this call.

5. Hand the same inputs to seldon
---------------------------------

.. code:: console

    $ ljos-consensus settle --ballots '[...]' --trust '[...]' --seldon --out run/

Writes ``config.toml``, ``network.txt`` and ``opinions.txt`` in Seldon's format,
runs ``seldon`` when it is on ``PATH``, and parses the final opinions back into
the same JSON shape. The discrete step and the differential-equation integration agree on the
fixed point; Seldon is there to check that they do.
