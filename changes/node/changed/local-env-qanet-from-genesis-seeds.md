# Enable qanet --from-genesis by exposing its validator seed references

`npm run run:qanet -- --from-genesis` always failed with "From-genesis mode
needs at least one validator seed phrase, but none of the seed env vars are set
()" even when every `MIDNIGHT_NODE_*_SEED` was provided. From-genesis discovers
validators by reading each service's `SEED_PHRASE` reference from the network
compose file, but qanet was the only well-known network with all of its
`SEED_PHRASE` entries commented out, so no validators were ever discovered and
the check threw with an empty (and misleading) variable list.

qanet's `SEED_PHRASE: $MIDNIGHT_NODE_<nn>_0_SEED` entries (node1-node12) are now
active, matching every other well-known network. The node still consumes keys
only from the generated `*_SEED_FILE` mounts, not `SEED_PHRASE`, so this changes
nothing for snapshot/fork bring-up; the phrases themselves are still supplied at
run time via `--env-file` or the environment.

PR:
Issue:
