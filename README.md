# consensus

DeGroot / Friedkin–Johnsen over a trust graph. The iteration is the same
one as `Seldon::DeGrootModel` in [seldon-code/seldon](https://github.com/seldon-code/seldon)
(`seldon_degroot_settle`). Set `SELDON_SRC` to that tree so `build.rs`
compiles `src/capi.cpp`. Without it, only the discrete FJ stepper runs.

`ljos-consensus --seldon` still execs the `seldon` binary when you want
the full TOML/network path.

```
ljos-consensus settle --ballots '[{"agent":"a","choice":"ship"},{"agent":"b","choice":"hold"}]'
```
