#node #security
# Consensus seed files read via the hardened file reader

`aura_seed_file`, `grandpa_seed_file`, and `cross_chain_seed_file` are now
read through `validated_file::safe_read_to_string` (regular-file check,
4 KiB size cap) instead of a plain `fs::read_to_string`. Symlinks remain
allowed because Kubernetes secret mounts expose files as symlinks into
`..data/`.

The `validated_file` module moved from the node crate to
`midnight-node-ledger-helpers` so the toolkit can reuse it for its new
`--*-file` secret arguments; the node re-exports it at the old path.

PR:
