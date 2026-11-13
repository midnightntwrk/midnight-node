#toolkit #fix
# Fix `root-call --encoded-call` panicking on any input

`root-call --encoded-call 0x…` always panicked with clap's "Mismatch between definition and access
of `encoded_call`". The derive read the `Option<Vec<u8>>` field as a list of `u8` values while the
hex parser produced a single `Vec<u8>`. `--encoded-call` now parses as a single hex value.
`--encoded-call-file` was unaffected.
