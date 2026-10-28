#node #local-env

# Add --num-validators to run a smaller mock authority set in local-env forks

`--num-validators <count>` lets a well-known network fork come up with fewer
mock validators than the network's Compose topology defines, instead of always
materializing the full set. It is threaded through the local-env commands that
bring up or roll a forked network (`run`, `image-upgrade`, `full-upgrade`, and
the runtime-upgrade path).

When forking from a snapshot, the count drives `mock-authorities convert` (how
many validator keysets are synthesized) and a generated Compose override that
starts only the selected `node*` services and disables the rest. The active
selection is persisted alongside the override, so later restarts can omit the
flag and reuse it.

The count must be a positive integer no larger than the network's
`mock.validatorServices` list. Changing it requires `--from-snapshot` so the
authority set and seeds can be regenerated — it is rejected on reuse runs and
in `--from-genesis` mode. The option is not supported by the standalone
`local-env` stack, whose five-validator topology and keys are fixed.

PR: https://github.com/midnightntwrk/midnight-node/pull/2008
