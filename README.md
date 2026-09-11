# consensus

DeGroot / Friedkin–Johnsen over a trust graph. **Seldon** ([seldon-code/seldon](https://github.com/seldon-code/seldon)) is the ODE engine. This crate does not link it (GPL). `ljos-consensus --seldon` execs `seldon` when it is on PATH.

`vissue consensus` stays the tracker verb. This crate is the model. `ljos consensus` calls here first.

```
ljos-consensus settle --ballots '[{"agent":"a","choice":"ship"},{"agent":"b","choice":"hold"}]'
```
