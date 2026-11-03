When the tracker read fails, `surprising --issue` now exits like `settle --issue`.
It gives the same `sysexits.h` status and the same stderr line.
A missing `vissue` is 69, a run that does not finish is 74, text that is not ballot JSON is 65, and an empty list is 66.
