`settle --engine energy` keeps its stop bar at a gradient of 2^-26 or above, which it already counts as settled.
A `--tol` far under the default no longer keeps it stepping past settled points.
`measurements/settle_scaling.txt` reports medians and interquartile ranges over seven fresh-process draws.
`measurements/settle_budget.txt` counts the settles that take the whole cap.
