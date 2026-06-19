# Add governance runtime upgrade option to fork-network workflow

Extend the `fork-network` GitHub Actions workflow with an `upgrade_mode`
input choosing between three upgrade shapes against the forked network:
`image` (existing behavior: roll validators to `new_node_image`),
`runtime` (submit the runtime wasm extracted from `new_node_image`
through the federated-authority governance flow via
`npm run governance-runtime-upgrade:${NETWORK}`), and `full` (image
rollout followed by the runtime upgrade via
`npm run full-upgrade:${NETWORK}`). An `allow_same_version` input maps
to the tooling's `--allow-same-version` rehearsal escape hatch.

The runtime and full modes extract the candidate runtime wasm from the
new node image's `/artifacts-<arch>/` directory and, after the upgrade, assert
the on-chain `:code` is byte-identical to it. The default
`mock_authorities_tag` is bumped to `368fd98`, the first published build
that rewrites Council/Technical Committee membership onto the
deterministic dev-keyring accounts the workflow signs with.

PR: https://github.com/midnightntwrk/midnight-node/pull/1676
Issue: https://github.com/midnightntwrk/midnight-node/issues/1468
