# Testing Upgrades

Entrypoint for upgrade testing tooling is in scripts/upgrade_test.sh.

## Purpose
The upgrade tooling informs on any incompatibilities between old and new states of the node and runtime. It aims to provide as close of an emulation of a real environment upgrade as possible by using the node images themselves as opposed to just modelling the state transition.

The upgrade tooling should be used on every change to the node which:
1. Alters the node client
2. Alters the node's runtime

## Important Note on Hardfork Testing

**The hardfork testing process is currently incomplete.** It was partially rewritten before the ledger v6 upgrade and never completed. For now, use the general upgrade testing approach described in this document.

## Scenarios
The upgrade scripts should be used in the following scenarios:
1. Runtime upgrade
2. Node upgrade
3. Runtime upgrade followed by node upgrade
4. Node upgrade followed by a runtime upgrade

What it does:
1. Fork important state(non-consensus/system) of a live environment
2. spin up a local network(docker)
3. Attempt your given upgrade type against the network
4. Leave you with an upgraded network which you can then then test/observe/break

## Dependencies
Subalfred - the tool attempts to install subalfred if you do not already have it.

## Usage: 
The tools require a number of environment variables in order to function:

`CURRENT_IMAGE`: The image tag(including container registry+repository) which the local network should be launched with.
`NEW_IMAGE`: The image tag(including container registry+repository) representing the image you want to upgrade the local network to.
`CHAIN_SPEC`: A raw JSON file containing the genesis of your environment. This will be merged with a new json chain spec containing forked state of the live environment.
`WASM_PATH`: The path to the WASM file of the runtime you would like to upgrade the local network to.
`NODE_URL`: The full ws/wss url to reach a node. This is used to request the full state of the node by the fork script.
`SUDO_KEY`: The privileged sudo key for the given environment.

## Common Upgrade-Related Commands

### Rebuild Metadata

When making changes that affect the runtime interface (new pallets, changed extrinsics, etc.), you need to regenerate the metadata:

```bash
earthly -P +rebuild-metadata
```

This updates the metadata that clients (wallets, indexers, applications) use to interact with the runtime.

**When to use:**
- After adding new pallets
- After changing extrinsic signatures
- After modifying runtime storage items
- During ledger version upgrades

### Rebuild Genesis

When the runtime storage format changes, you may need to regenerate the genesis state:

```bash
earthly -P +rebuild-genesis
```

**When to use:**
- After changing genesis configuration
- During major runtime upgrades
- After adding new pallets with genesis configuration
- During ledger version upgrades

**Note:** For deployed networks (qanet, preview, testnet), this requires AWS secrets. If you don't have access, ask the node team to run this command after downloading the secrets.

## Ledger Upgrade Process

When upgrading the midnight-ledger dependency:

1. Update `Cargo.toml` with new ledger version
2. Run `cargo check` to identify compilation errors
3. Fix any API changes or breaking changes
4. Run `cargo test` to ensure tests pass
5. Run `earthly -P +rebuild-metadata` to update metadata
6. Run `earthly -P +rebuild-genesis` if storage format changed
7. Test the upgrade using the upgrade testing tools described above

See [development-workflow.md](development-workflow.md) for more details on ledger upgrades.
