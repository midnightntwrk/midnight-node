#node
# Default `unsafe_allow_symlinks` to `false` when missing from config

Add `#[serde(default)]` to `MetaCfg::unsafe_allow_symlinks` so the field
falls back to `false` instead of producing a `missing field` config error
at startup. This restores compatibility for deployments running a new node
binary against an older `default.toml` that predates the field.

PR: https://github.com/midnightntwrk/midnight-node/pull/1600
Issue: https://github.com/midnightntwrk/midnight-node/issues/1599
