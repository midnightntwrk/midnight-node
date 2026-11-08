#node #config
# Default `block_stability_margin` to 30 across all config presets

Raise `block_stability_margin` from 10 to 30 in `res/cfg/default.toml` (the
fallback inherited by dev/devnet/govnet/guardnet/local/perfnet/preview/qanet/
stagenet) and in the two presets that pin it explicitly, `res/cfg/preprod.toml`
and `res/cfg/mainnet.toml`.

`block_stability_margin` adds to `cardano_security_parameter` when selecting the
latest stable Cardano block a producer references (`tip − (k + margin)`). It is
an operator-side safety setting; the on-chain effective margin is the minimum
across all producers (the `mcsh` reference is monotonic and cannot regress), so
a uniform default is required for it to take effect network-wide. This change
makes 30 the built-in default so operators using a preset without an explicit
`BLOCK_STABILITY_MARGIN` env override get the more conservative Cardano-reference
lag by default.

Note: this proposes changing the mainnet preset as well; the mainnet value should
only take effect via a deliberate, coordinated node rollout.

PR: https://github.com/midnightntwrk/midnight-node/pull/1914
Issue: https://github.com/shieldedtech/shielded-sre/issues/424
