Settle the tracker's ballots under the pack's rows
==================================================

The seat does this in one verb:

.. code:: console

   $ ljos consensus proj-1a2b

It reads the live ``trust`` atoms from the pack and passes them as ``--trust``
to this crate and to the tracker's own ``consensus`` verb. The two settles
weigh one graph.

Feed rows from anywhere
=======================

Rows are JSON, tuples or objects:

.. code:: text

   $ ljos-consensus settle --issue proj-1a2b --trust '[["alice","carol",0.9],{"from":"carol","to":"alice","weight":0.5}]'

A row from a voter to itself sets its self weight; a voter with no row of
its own listens to everyone equally, itself included, as the tracker's
default does.

Anchor the voters
=================

.. code:: console

   $ ljos-consensus settle --issue proj-1a2b --susceptibility 0.7

Below one, each voter keeps a share of its own ballot. The run always
settles; the shares report where the group's weight ended up.

Read the answer in a script
===========================

.. code:: console

   $ ljos-consensus settle --issue proj-1a2b | jq -r '.options[.shares | index(max)]'

``settled`` is false when the budget ran out; ``engine`` says which model
answered.

Cross-check with Seldon
=======================

.. code:: console

   $ ljos-consensus settle --issue proj-1a2b --seldon --out run/

Writes ``config.toml``, ``network.txt`` and ``opinions.txt``, runs ``seldon`` when
it is on ``PATH``, and reads the final opinions back. The parse path does
not print a residual. When ``seldon`` is absent the discrete step is the
settle.
