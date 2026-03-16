# Configuration Guide

## Configuration sources

Configuration can be loaded either from and are applied in the following order:
(later sources override earlier)

- Default values: stored in `res/cfg/default.toml` (Midnight + Substrate)
- Configuration Preset files: stored in `res/cfg/<preset>.toml`, loaded at runtime (Midnight + Substrate)
- Environment Variables (Midnight + Substrate)
- CLI arguments (Substrate-only)

For example, if `default.toml` sets `validator = false` and you set `VALIDATOR=1` in the environment, the node runs as a validator.

The CLI supports the same arguments as Substrate/PolkadotSDK-based nodes. Some commonly-used Substrate variables can be set via our env-var config system. Midnight-specific variables are all set via default values, env-vars or config preset files.

### Environment variable naming

Config keys use `snake_case` in TOML files. Environment variables are case-insensitive.

| TOML key | Environment variable |
|----------|----------------------|
| `validator` | `VALIDATOR` |
| `cardano_security_parameter` | `CARDANO_SECURITY_PARAMETER` |
| `mc__first_epoch_timestamp_millis` | `MC__FIRST_EPOCH_TIMESTAMP_MILLIS` |

Double underscores (`__`) denote nested configuration groups.

Boolean values accept any truthy value: `1`, `true`, `TRUE`, `True`, etc.

## Inspecting configuration

When run with `SHOW_CONFIG=1`, the node will print all it's configuration values, including a short description of each, and the source of the value i.e. where the configuration was loaded from. Example:

```sh
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

To run the node, you must supply a chainspec file. Chainspec files for known networks are stored in `res/<network-name>/` and are named `chain-spec.json` (human-readable) or `chain-spec-raw.json` (encoded for production use).

The raw chainspec can be generated from `chain-spec.json`, and contains the raw storage values for the node genesis.

**Raw vs non-raw chainspecs:**

- **Non-raw (plain)**: Human-readable keys and values (e.g., `"sudo": { "key": "5Grwva..." }`). Used for editing and customization.
- **Raw**: Encoded storage keys suitable for the Substrate storage trie. Required for production deployment and syncing after runtime upgrades.

Always distribute **raw** chainspecs to production nodes. Use non-raw specs only for inspection or modification.

To generate a chainspec, you need all the `chainspec_` config values defined:

```sh
$ docker run --rm -e SHOW_CONFIG=1 midnightntwrk/midnight-node:latest-main 2>&1 | rg 'NAME:.*chainspec_.*$'
NAME:          chainspec_name
NAME:          chainspec_id
NAME:          chainspec_genesis_state
NAME:          chainspec_genesis_block
NAME:          chainspec_chain_type
NAME:          chainspec_pc_chain_config
NAME:          chainspec_cnight_genesis
NAME:          chainspec_federated_authority_config
NAME:          chainspec_system_parameters_config
NAME:          chainspec_permissioned_candidates_config
NAME:          chainspec_registered_candidates_addresses
NAME:          chainspec_ics_config
```

Once all those config values are defined, running the node with `build-spec` will export the chainspec:

```sh
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
chainspec_cnight_genesis = "res/qanet/cnight-config.json"
chainspec_federated_authority_config = "res/qanet/federated-authority-config.json"
chainspec_system_parameters_config = "res/qanet/system-parameters-config.json"
chainspec_permissioned_candidates_config = "res/qanet/permissioned-candidates-config.json"
chainspec_registered_candidates_addresses = "res/qanet/registered-candidates-addresses.json"
chainspec_ics_config = "res/qanet/ics-config.json"
```

The process for building chainspecs is automated via Earthly build commands:

```sh
$ earthly +rebuild-chainspec --NETWORK=<network>
$ earthly +rebuild-all-chainspecs
```

For a complete guide on genesis generation workflow, including the dependency sequence between config files, ledger state, and chainspec generation, see the [Genesis Generation Guide](genesis/README.md).

## `genesis_state_<network>.mn` and `genesis_block_<network>.mn`: Building Ledger state

Each chain requires a genesis ledger state. All test networks contain a set of seeds pre-funded with NIGHT, Shielded tokens, and DUST. To generate genesis for these test networks, we must have the genesis seeds for the networks on the filesystem.

**Important:** Before generating ledger state, you must first generate the config files (`cnight-config.json`, `ics-config.json`) from their corresponding address files. See [Genesis Generation Guide](genesis/README.md) for the complete dependency sequence.

The exception to this is the `undeployed` network, which uses the following well-known seeds:

```json
{
    "wallet-seed-0": "0000000000000000000000000000000000000000000000000000000000000001",
    "wallet-seed-1": "0000000000000000000000000000000000000000000000000000000000000002",
    "wallet-seed-2": "0000000000000000000000000000000000000000000000000000000000000003",
    "wallet-seed-3": "a51c86de32d0791f7cffc3bdff1abd9bb54987f0ed5effc30c936dddbb9afd9d530c8db445e4f2d3ea42a321b260e022aadf05987c9a67ec7b6b6ca1d0593ec9"
}
```

Genesis is rebuilt using the toolkit's `generate-genesis` command:

```sh
$ docker run --rm midnightntwrk/midnight-node-toolkit:latest-main generate-genesis --network qanet --seeds-file genesis-seeds-qanet.json
```

This process is automated via Earthly build commands:

```sh
$ earthly +rebuild-genesis-state-<network>
$ earthly +rebuild-all-genesis-states
```

New seeds can be generated via Earthly too - the generated file is written to `./secrets/`:

```sh
$ earthly +generate-seeds --NETWORK=<network> --OUTPUT_FILE=<network>-genesis-seeds.json
```

## `pc-chain-config.json`: PartnerChains Configuration

The `pc-chain-config.json` is an output of the PartnerChains chain initialisation. See the [Partner Chains Chain Builder Documentation](https://github.com/input-output-hk/partner-chains/blob/898ee1cb082dd1002afdd8bcf01b4aee494c03f3/docs/user-guides/chain-builder.md#storing-the-main-chain-configuration) for more information on this.

We use the `initial_authorities` field as the initial committee for the node. After the first epoch, the committee is loaded via the Ariadne selection algorithm from the list of registered and permissioned nodes indexed from the connected Cardano chain.

## `cnight-config.json`

Contains mappings between Cardano and Dust addresses, and which addresses the cnight main-chain-follower should track.

The addresses in this file are stateless - all networks connected to Cardano preview should use the same `cnight-config.json` file, unless the network needs a different set of cNight mappings (advanced usage).

The `cnight-config.json` file is generated using the `generate-c-night-genesis` command on the node:

```sh
$ docker run --rm midnightntwrk/midnight-node:latest-main generate-c-night-genesis -h
```

When `CFG_PRESET` is set, the command uses default paths:
- `--cnight-addresses` defaults to `res/<CFG_PRESET>/cnight-addresses.json`
- `--output` defaults to `res/<CFG_PRESET>/cnight-config.json`

## `ics-config.json`

Contains the Illiquid Circulation Supply (ICS) configuration for treasury funding. This file tracks cNIGHT tokens locked in the ICS validator contract on Cardano, which determines the initial treasury allocation at genesis.

The file includes:
- `illiquid_circulation_supply_validator_address`: The Cardano address of the ICS validator contract
- `asset`: The cNIGHT token identifier (policy_id and asset_name)
- `utxos`: List of observed UTXOs at the validator address
- `total_amount`: Total cNIGHT locked in the validator

Generate this file using the `generate-ics-genesis` command:

```sh
$ docker run --rm -e CFG_PRESET=qanet midnightntwrk/midnight-node:latest-main generate-ics-genesis --cardano-tip <block_hash>
```

When `CFG_PRESET` is set, the command uses default paths:
- `--ics-addresses` defaults to `res/<CFG_PRESET>/ics-addresses.json`
- `--output` defaults to `res/<CFG_PRESET>/ics-config.json`

## `ics-addresses.json`

Input file for `generate-ics-genesis`. Contains the ICS validator address and token identifier:

```json
{
    "illiquid_circulation_supply_validator_address": "<cardano_address>",
    "asset": {
        "policy_id": "<policy_id_hex>",
        "asset_name": "NIGHT"
    }
}
```

## `federated-authority-config.json`

This file contains the set of governance authorities for both the technical committee and the council. These values will vary across different chains if the governance authorities should differ.

Each collective (`council` and `technical_committee`) requires:

- `members`: Array of Substrate SS58 account IDs (hex-encoded)
- `members_mainchain`: Corresponding Cardano payment key hashes
- `address`: Cardano address for governance transactions
- `policy_id`: Minting policy ID for governance NFTs

Generate this file using the `generate-federated-authority-genesis` command:

```sh
$ docker run --rm -e CFG_PRESET=qanet midnightntwrk/midnight-node:latest-main generate-federated-authority-genesis --cardano-tip <block_hash>
```

When `CFG_PRESET` is set, the command uses default paths:
- `--federated-auth-addresses` defaults to `res/<CFG_PRESET>/federated-authority-addresses.json`
- `--output` defaults to `res/<CFG_PRESET>/federated-authority-config.json`

For test networks, you can also copy from an existing network (e.g., `res/qanet/federated-authority-config.json`) and update the member keys.

## `federated-authority-addresses.json`

Input file for `generate-federated-authority-genesis`. Contains the Cardano addresses and policy IDs for governance collectives:

```json
{
    "council_address": "<cardano_address>",
    "council_policy_id": "<policy_id_hex>",
    "technical_committee_address": "<cardano_address>",
    "technical_committee_policy_id": "<policy_id_hex>"
}
```

## `system-parameters-config.json`: Midnight Governance Parameters

Stores the terms and conditions for using the network, and the D parameter using in the Partner-chains Ariadne Selection Algorithm.

The D parameter should match the intended mix of permissioned and registered validators for the network. For example, a federated-only network should have `num_permissioned_candidates` >= the initial authorities (in `pc-chain-config.json`) and <= the epoch length (hard-coded to 300), and `num_registered_candidates` set to `0`. If registered nodes are expected, set `num_registered_candidates` higher to allow SPOs to occupy slots in the committee.

## `permissioned-candidates-config.json`

Contains the permissioned candidates policy ID and the list of initial permissioned candidates for the network. This file is used during chainspec generation to configure which permissioned validators can participate in consensus.

The file includes:
- `permissioned_candidates_policy_id`: The Cardano minting policy ID for permissioned candidate NFTs (hex with 0x prefix)
- `initial_permissioned_candidates`: Array of candidate entries, each with:
  - `sidechain_pub_key`: ECDSA public key for cross-chain signing
  - `aura_pub_key`: Sr25519 public key for block production
  - `grandpa_pub_key`: Ed25519 public key for block finalization
  - `beefy_pub_key`: ECDSA public key for BEEFY consensus

Generate this file using the `generate-permissioned-candidates-genesis` command:

```sh
$ docker run --rm -e CFG_PRESET=qanet midnightntwrk/midnight-node:latest-main generate-permissioned-candidates-genesis --cardano-tip <block_hash>
```

When `CFG_PRESET` is set, the command uses default paths:
- `--permissioned-candidates-addresses` defaults to `res/<CFG_PRESET>/permissioned-candidates-addresses.json`
- `--output` defaults to `res/<CFG_PRESET>/permissioned-candidates-config.json`

## `permissioned-candidates-addresses.json`

Input file for `generate-permissioned-candidates-genesis`. Contains the Cardano policy ID to query for permissioned candidate registrations:

```json
{
    "permissioned_candidates_policy_id": "<policy_id_hex>"
}
```

## `registered-candidates-addresses.json`

Contains the Cardano address used to track registered candidate (SPO) registrations:

```json
{
    "committee_candidates_address": "<cardano_address>"
}
```

This address is monitored by the main-chain-follower to detect when SPOs register as validators.

## Generating All Genesis Configs

To generate all genesis configuration files at once, use the `generate-genesis-config` command:

```sh
$ docker run --rm -e CFG_PRESET=qanet midnightntwrk/midnight-node:latest-main generate-genesis-config --cardano-tip <block_hash>
```

This command generates:
- `cnight-config.json`
- `ics-config.json`
- `federated-authority-config.json`
- `permissioned-candidates-config.json`

All output paths default to `res/<CFG_PRESET>/` when `CFG_PRESET` is set.

For an interactive guided experience, use the genesis generation script:

```sh
$ ./scripts/genesis/genesis-construction.sh
```

See the [Genesis Generation Guide](genesis/README.md) for complete documentation.

## Validator keys

Validator nodes require secret keys for consensus participation. These are configured via environment variables pointing to key files:

| Environment variable | Purpose | Key type |
| -------------------- | ------- | -------- |
| `AURA_KEY_FILE` | Block production (AURA consensus) | [Sr25519](https://github.com/w3f/polkadot-wiki/blob/61105e5b014aca11900aae7df68348803ebd4cc6/docs/learn/learn-cryptography.md?plain=1#L22) |
| `GRANDPA_KEY_FILE` | Block finalization (GRANDPA consensus) | [Ed25519](https://en.wikipedia.org/wiki/EdDSA#Ed25519) |
| `CROSS_CHAIN_KEY_FILE` | Cross-chain signing | [EdDSA](http://en.wikipedia.org/wiki/EdDSA) |
| `BEEFY_KEY_FILE` | Aggregated finalisation proof | [EdDSA](http://en.wikipedia.org/wiki/EdDSA) |

Each file should contain a secret seed for the respective key type. The public keys derived from these seeds must match an entry in `initial_authorities` (in `pc-chain-config.json`) for the node to participate in consensus.

**Block production requirements:**

- For a network to **produce blocks**, at least one validator with valid AURA keys must be online
- For a network to **finalize blocks**, a 2/3 supermajority of `initial_authorities` must be connected with valid GRANDPA keys

If blocks are being produced but not finalized, check that enough validators are online and their keys match the `initial_authorities` configuration.

## Passing Substrate CLI arguments

Substrate-native CLI arguments can be passed via the `args` or `append_args` config keys:

```toml
# In preset file - replaces all default args
args = ["--rpc-external", "--rpc-cors=all"]

# Or append to existing args
append_args = ["--prometheus-external"]
```

Common Substrate flags for SREs:

- `--state-pruning archive` - Keep full state history
- `--blocks-pruning archive` - Keep all blocks
- `--rpc-external` - Expose RPC to external connections
- `--prometheus-external` - Expose metrics endpoint

See `midnight-node --help` for all available options.

## Memory Monitoring

The node includes a memory monitor that periodically checks available system memory and triggers a graceful shutdown before the Linux OOM killer intervenes. This is particularly relevant during initial blockchain synchronization, which can consume significant memory.

### Configuration

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `memory_threshold` | `u64` | `0` (disabled) | Minimum available memory in MiB. Node shuts down if available memory drops below this value. |
| `memory_polling_period` | `u32` | `1` | How often to check available memory, in seconds. |

Set via TOML config, environment variables, or CLI flags (`--memory-threshold`, `--memory-polling-period`):

```toml
# In preset or default.toml
memory_threshold = 512
memory_polling_period = 5
```

```sh
# Via environment
MEMORY_THRESHOLD=512 MEMORY_POLLING_PERIOD=5 ./midnight-node
```

### Memory source detection

On Linux, the monitor detects the memory source once at startup:

1. **cgroup v2** — `memory.max` and `memory.current` under `/sys/fs/cgroup/`. Used when running in Docker or Kubernetes with memory limits.
2. **cgroup v1** — `memory.limit_in_bytes` and `memory.usage_in_bytes` under `/sys/fs/cgroup/memory/`. Used with older container runtimes.
3. **`/proc/meminfo`** — `MemAvailable` field. Used on bare metal or when no cgroup memory limit is set.

Unlimited cgroup limits (`memory.max = "max"` for v2, or `limit_in_bytes > 2^62` for v1) are detected and the monitor falls through to the next source.

On non-Linux platforms, the memory monitor is not supported and logs a warning at startup.

### Recommended thresholds

The appropriate threshold depends on the deployment environment. A value of `512` MiB (matching the storage monitor's default) is a reasonable starting point. For nodes synchronizing large chains, consider a higher threshold (e.g., `1024`–`2048` MiB) to allow headroom for memory spikes during sync.

A warning is logged when available memory drops below 2x the threshold, providing early notice before shutdown.

## Troubleshooting

### Diagnosing configuration issues

1. **Always start with `SHOW_CONFIG=1`** to verify values and their sources
2. Check for typos in environment variable names
3. Verify `CFG_PRESET` matches an existing file in `res/cfg/`

### Common issues

| Symptom | Likely cause | Fix |
| ------- | ------------ | --- |
| Node fails to start with "chainspec not found" | Missing or incorrect `chain` config | Verify chainspec path exists and `CFG_PRESET` is set |
| "Genesis mismatch" when syncing | Wrong chainspec version | Ensure all nodes use identical `chain-spec-raw.json` |
| Node starts but won't produce blocks | Keys (`{AURA, GRANDPA, CROSS_CHAIN}_SEED_FILE`) don't match initial authorities. | Verify the secret keys for each node match `initial_authorities` |
