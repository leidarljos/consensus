.. raw:: html

   <div class="vi-hero">
     <div class="vi-hero-brand">
       <img class="vi-hero-mark" src="_static/mark.svg" width="64" height="64" alt="" />
       <div>
         <p class="vi-hero-name">consensus</p>
         <p class="vi-hero-tag">Who agrees, weighted by who listens to whom.</p>
       </div>
     </div>
     <p class="vi-hero-tagline">DeGroot and Friedkin-Johnsen settles over ballots and a trust graph.</p>
     <div class="vi-hero-pills">
       <span>Discrete model</span>
       <span>Seldon export</span>
       <span>JSON in, JSON out</span>
     </div>
     <div class="vi-hero-actions">
       <a class="vi-btn vi-btn-gold" href="getting-started.html">Get started</a>
       <a class="vi-btn vi-btn-ghost" href="reference.html">Reference</a>
     </div>
   </div>

A tally counts ballots. A settle weighs them: each voter listens to the
voters it trusts and moves toward their opinion. Where that iteration stops
is the group's position. ``ljos-consensus`` is the discrete model as one
binary: ballots and trust rows in, shares and rounds out. It can also write
the inputs for the Seldon opinion-dynamics engine and read its output.

Install
=======

.. code:: console

   $ cargo binstall ljos-consensus
   $ ljos-consensus settle --ballots '[{"agent":"a","choice":"ship"},{"agent":"b","choice":"hold"}]'

First minute
============

.. code:: console

   $ cargo binstall ljos-consensus
   $ ljos-consensus settle --ballots '[{"agent":"alice","choice":"ship"},{"agent":"bob","choice":"hold"}]'
   {"options":["hold","ship"],"shares":[0.5,0.5],"rounds":2,"settled":true,"residual":0.0,"engine":"degroot-fj","polarization":0.0,"disagreement":0.0}

The :doc:`tutorial <getting-started>` adds trust, then an anchor.
The seat verb is ``ljos consensus``.

.. toctree::
   :maxdepth: 1
   :caption: Guides
   :hidden:

   getting-started
   howto
   reference
   explanation
   seat
