# Fix regression in `wizards generate-keys` command

The initial fix from PR #187 introduced a regression where running `wizards generate-keys` without any configuration would panic with `called Option::unwrap() on a None value` at `node/src/cfg/mod.rs:100:88`.

This occurred because the fix was incomplete - it addressed the issue for explicit `--chain-id` usage but introduced a new panic by calling `load_spec("")` when using configuration presets (CFG_PRESET environment variable).

Fixed by making `create_chain_spec()` properly respect the configuration system:
- Uses the `chain` value from `SubstrateCfg` (configurable via `CHAIN` env var or `CFG_PRESET`)
- Defaults to `"dev"` when no chain is specified

Now supports all usage patterns:
- `./midnight-node wizards generate-keys` (defaults to dev)
- `CFG_PRESET=qanet ./midnight-node wizards generate-keys` 
- `CHAIN=local ./midnight-node wizards generate-keys`
- `CHAIN=path/to/chain-spec.json ./midnight-node wizards generate-keys`

Related to PR: https://github.com/midnightntwrk/midnight-node/pull/187
PR: https://github.com/midnightntwrk/midnight-node/pull/365