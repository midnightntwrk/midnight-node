# Restore from-genesis bring-up for well-known networks in local-env tooling

`npm run run:<network> -- --from-genesis` brings up a well-known network's base
compose from block 0 again, instead of requiring `--from-snapshot` (the genesis
path was removed together with the internal k8s/AWS integration in #1470).
Nothing is mocked in this mode: validator seed phrases and a main-chain data
source must be supplied via `--env-file` or the environment, and the CLI warns
up front about compose variables left unset and about pre-existing chain data
directories.

Also clarifies the fork-network workflow's `node_image`/`new_node_image` inputs
(full Docker image references with the expected release and CI tag formats, not
git branches/shas/URLs) and documents how to find snapshot archives for each
network from the backup system's public index in `docs/fork-testing.md`.

PR:
Issue:
