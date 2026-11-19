A negative number after a flag is read as its value.
`settle --self-weight -1` now prints the range error for that flag, where it printed an unexpected argument named `-1`.
`reputation --alpha -inf` is refused as not finite, as `--alpha=-inf` is.
