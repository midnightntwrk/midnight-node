#node #chainspec #preview

# Add Preview boot nodes to the Preview chain spec

A node started with `CFG_PRESET=preview` and no `--bootnodes` used to stay at
`Idle (0 peers)`, because the Preview chain spec shipped with an empty
`bootNodes` list. `chain-spec.json`, `chain-spec-abridged.json` and
`chain-spec-raw.json` under `res/preview/` now list `bootnode-1.preview` and
`bootnode-2.preview`, as the Preprod and Mainnet specs already do.

Only `bootNodes` changes. Genesis is byte-identical, so the genesis hash is
unaffected. `res/preview/bootnodes-config.json` is added too, so the genesis
construction wizard keeps them on a future rebuild.

PR: https://github.com/midnightntwrk/midnight-node/pull/2346
Issue: https://github.com/midnightntwrk/midnight-node/issues/2236
