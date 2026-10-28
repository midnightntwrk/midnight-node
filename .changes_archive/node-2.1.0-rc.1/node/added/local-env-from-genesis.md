# Restore from-genesis bring-up for well-known networks in local-env tooling

`npm run run:<network> -- --from-genesis` brings up a well-known network's base
compose from block 0 again, instead of requiring `--from-snapshot` (the genesis
path was removed together with the internal k8s/AWS integration in #1470).
Nothing is mocked in this mode: validator seed phrases and a main-chain data
source must be supplied via `--env-file` or the environment, and the CLI warns
up front about compose variables left unset and about pre-existing chain data
directories.

Provided seed phrases are wired into node keystores through a generated
`<network>.genesis.override.yaml` that mounts per-validator seed files and sets
`AURA_SEED_FILE`/`GRANDPA_SEED_FILE`/`CROSS_CHAIN_SEED_FILE` — the node does
not consume the `SEED_PHRASE` env var the base compose files pass. Networks
deployed with a distinct phrase per key type can set per-type vars instead
(`<VAR>_AURA_SEED`/`<VAR>_GRANDPA_SEED`/`<VAR>_CROSS_CHAIN_SEED` replacing the
base var's `_SEED` suffix, with the base var as fallback). A
`--compose-override` option lets fully local runs layer extra compose config on
top, such as enabling the node's built-in mock main-chain follower.

Also clarifies the fork-network workflow's `node_image`/`new_node_image` inputs
(full Docker image references with the expected release and CI tag formats, not
git branches/shas/URLs) and documents how to find snapshot archives for each
network from the backup system's public index in `docs/fork-testing.md`.

PR: https://github.com/midnightntwrk/midnight-node/pull/1807
Issue: https://github.com/midnightntwrk/midnight-node/issues/1468
