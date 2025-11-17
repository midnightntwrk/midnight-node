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

PR: https://github.com/midnightntwrk/midnight-node/pull/187
