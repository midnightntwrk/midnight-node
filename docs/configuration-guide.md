# Configuration Guide

## Configuration sources

Configuration can be loaded either from:

- Default values: stored in `res/cfg/default.toml` (Midnight + Substrate)
- Environment Variables (Midnight + Substrate)
- Configuration Preset files: stored in `res/cfg/<preset>.toml`, loaded at runtime (Midnight + Substrate)
- CLI arguments (Substrate-only)

The CLI supports the same arguments as Substrate/PolkadotSDK-based nodes. Some commonly-used Substrate variables can be set via our env-var config system. Midnight-specific variables are all set via default values, env-vars or config preset files.

## Inspecting configuration

When run with `SHOW_CONFIG=1`, the node will print all it's configuration values, including a short description of each, and the source of the value i.e. where the configuration was loaded from. Example:

```
$ docker run --rm -e CFG_PRESET=dev -e CHAINSPEC_ID=my_new_chain_id -e SHOW_CONFIG=1 midnightntwrk/midnight-node:latest-main 

================================================================================
ChainSpecCfg
================================================================================

NAME:          chainspec_name
HELP:          Required for generic Live network chain spec
               Name of the network e.g. devnet1
TYPE:          Option < String >
DEFAULT:       
SOURCES:       preset
CURRENT_VALUE: Midnight Undeployed

NAME:          chainspec_id
HELP:          Required for generic Live network chain spec
               Id of the network e.g. devnet
TYPE:          Option < String >
DEFAULT:       
SOURCES:       env-vars
CURRENT_VALUE: my_new_chain_id

...
```

## Chainspecs

To run the node, you must supply a chainspec file. Chainspec files for known networks are stored in `res/<network-name>/` and are either `chainspec.json` or `chainspec-raw.json`. 

The raw chainspec can be generated from the chainspec.json, and contains the raw storage values for the node genesis. # Claude: give short summary on the differences between raw and not-raw

To generate a chainspec, you need all the `chainspec_` config values defined:

```
$ docker run --rm -e SHOW_CONFIG=1 midnightntwrk/midnight-node:latest-main 2>&1 | rg 'NAME:.*chainspec_.*$'
NAME:          chainspec_name
NAME:          chainspec_id
NAME:          chainspec_genesis_state
NAME:          chainspec_genesis_block
NAME:          chainspec_chain_type
NAME:          chainspec_pc_chain_config
NAME:          chainspec_cnight_genesis
NAME:          chainspec_federated_authority_config
```

Once all those config values are defined, running the node with `build-spec` will export the chainspec:

```
$ docker run --rm -e CFG_PRESET=qanet midnightntwrk/midnight-node:latest-main build-spec
...
```

This works because the `res/cfg/qanet.toml` config preset has all the `chainspec_` variables defined.

`qanet.toml`:
```toml
...
chainspec_name = "Midnight QANet"
chainspec_id = "midnight_qanet"
chainspec_genesis_state = "res/genesis/genesis_state_qanet.mn"
chainspec_genesis_block = "res/genesis/genesis_block_qanet.mn"
chainspec_chain_type = "live"
chainspec_pc_chain_config = "res/qanet/pc-chain-config.json"
chainspec_cnight_genesis = "res/qanet/cnight-genesis.json"
chainspec_federated_authority_config = "res/qanet/federated-authority-config.json"
chainspec_system_parameters_config = "res/qanet/system-parameters-config.json"
```


## `genesis_state_<network>mn` and `genesis_block_<network>.mn`: Building Ledger state

Each chain requires a genesis ledger state. All test networks contain a set of seeds pre-funded with NIGHT, Shielded tokens, and DUST. To generate genesis for these test networks, we must have the genesis seeds for the networks on the filesystem.

The exception to this is the `undeployed` network, which uses the following well-known seeds:
```
{
    "wallet-seed-0": "0000000000000000000000000000000000000000000000000000000000000001",
    "wallet-seed-1": "0000000000000000000000000000000000000000000000000000000000000002",
    "wallet-seed-2": "0000000000000000000000000000000000000000000000000000000000000003",
    "wallet-seed-3": "a51c86de32d0791f7cffc3bdff1abd9bb54987f0ed5effc30c936dddbb9afd9d530c8db445e4f2d3ea42a321b260e022aadf05987c9a67ec7b6b6ca1d0593ec9"
}
```

Genesis is rebuilt using the toolkit's `generate-genesis` command:

```
$ docker run --rm midnightntwrk/midnight-node-toolkit:latest-main generate-genesis --network qanet --seeds-file genesis-seeds-qanet.json
```

This process is automated via Earthly build commands:
```
$ earthly +rebuild-genesis-state-<network>
$ earthly +rebuild-all-genesis-states
```


## `pc-chain-config.json`: PartnerChains Configuration

The `pc-chain-config.json` is an output of the PartnerChains chain initialisation. See the [Partner Chains Chain Builder Documentation](https://github.com/input-output-hk/partner-chains/blob/898ee1cb082dd1002afdd8bcf01b4aee494c03f3/docs/user-guides/chain-builder.md#storing-the-main-chain-configuration) for more information on this.

We use the `initial_authorities` field as the initial committee for the node. After the first epoch, the committee is loaded via the Ariadne selection algorithm from the list of registered and permissioned nodes indexed from the connected Cardano chain.

## `cnight-genesis.json`

Contains mappings between Cardano and Dust addresses, and which addresses the cnight main-chain-follower should track.

The addresses in this file are stateless - all networks connected to Cardano preview should use the same `cnight-genesis.json` file, unless the network needs a different set of cNight mappings (advanced usage).

The `cnight-genesis.json` file is generated using the `generate-c-night-genesis` command on the node:

```
$ docker run --rm midnightntwrk/midnight-node:latest-main generate-c-night-genesis -h
```

## `federated-authority-config.json`

This file contains the set of governance authorities for both the technical committee and the council. These values will vary across different chains if the governance authorities should differ.

# TODO: How do we generate this file?

## `system-parameters-config.json`: Midnight Governance Parameters

Stores the terms and conditions for using the network, and the D parameter using in the Partner-chains Ariadne Selection Algorithm. 

The D parameter should match the intended mix of permissioned and registered validators for the network. For example, a federated-only network should have `num_permissioned_candidates` >= the initial authorities (in `pc-chain-config.json`) and <= the epoch length (hard-coded to 300), and `num_registered_candidates` set to `0`. If registered nodes are expected, set `num_registered_candidates` higher to allow SPOs to occupy slots in the committee.

