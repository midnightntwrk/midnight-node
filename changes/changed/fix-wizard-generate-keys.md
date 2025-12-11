# Fix `wizards generate-keys` command for non-dev chains 

Previously, this command resulted in an error:

```
CFG_PRESET=node-dev-01 ./midnight-node wizards generate-keys
This 🧙 wizard will generate the following keys and save them to your node's keystore:
→ ecdsa Cross-chain key
→ sr25519 AURA key
→ ed25519 Grandpa key
→ ecdsa Cross-chain key
It will also generate a network key for your node if needed.


thread 'main' panicked at node/src/cli.rs:113:14:
chain spec generation must succeed when using default configuration: "ChainSpec Parse error: Error opening spec file `node-dev-01`: No such file or directory (os error 2)"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

Initial fix PR: https://github.com/midnightntwrk/midnight-node/pull/187

**Additional fix**: The initial fix introduced a regression where running `wizards generate-keys` without any configuration would panic with `called Option::unwrap() on a None value` at `node/src/cfg/mod.rs:100:88`. 

Fixed by making `create_chain_spec()` properly respect the configuration system:
- Uses the `chain` value from `SubstrateCfg` (configurable via `CHAIN` env var or `CFG_PRESET`)
- Defaults to `"dev"` when no chain is specified

Now supports all usage patterns:
- `./midnight-node wizards generate-keys` (defaults to dev)
- `CFG_PRESET=qanet ./midnight-node wizards generate-keys` 
- `CHAIN=local ./midnight-node wizards generate-keys`
- `CHAIN=path/to/chain-spec.json ./midnight-node wizards generate-keys`
