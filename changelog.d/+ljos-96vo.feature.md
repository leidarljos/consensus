`shrunk_accuracy` reads a record of h hits in n as (h + 1) / (n + 2).
No record is then a certainty.
The trust floor is `f64::EPSILON`, kept only as a clamp for bare accuracies of zero or one.
The settle's tolerance is about 1.49e-8 and its round cap 27, both derived from binary64.
The settle command line's `--max-iter` and `--tol` default to `Opts::default` for two engines.
Those are the iterate and exact engines.
A settle that runs out of rounds above contraction 1/2 says so in `note`.
The energy engine drops its softmax gauge penalty.
