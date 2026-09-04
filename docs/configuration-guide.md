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

## Cardano db-sync compatibility

Midnight reads Cardano data from an existing cardano-db-sync PostgreSQL database. The node can
query both supported transaction-input representations and both supported address
representations. Schema management is configured separately, so the PostgreSQL role used by the
node can be read-only.

| TOML key | Environment variable | Values | Default |
|----------|----------------------|--------|---------|
| `db_sync_tx_input_mode` | `DB_SYNC_TX_INPUT_MODE` | `auto`, `tx_in`, `consumed` | `auto` |
| `db_sync_address_mode` | `DB_SYNC_ADDRESS_MODE` | `inline`, `address_table` | `inline` |
| `db_sync_schema_mode` | `DB_SYNC_SCHEMA_MODE` | `apply`, `verify`, `skip` | `apply` |

The defaults preserve the previous node behavior for initialized standard db-sync databases. An
empty schema whose input representation cannot be inferred now fails safely. For production
deployments, set the two layout options explicitly. Layout validation uses the connection's
current PostgreSQL `search_path`, so the selected db-sync tables must resolve without
schema-qualified names.

### Selecting the db-sync layout

`db_sync_tx_input_mode` maps to cardano-db-sync's `insert_options.tx_out` settings:

| Midnight mode | Required db-sync representation |
|---------------|---------------------------------|
| `tx_in` | A complete `tx_in` table with `tx_in_id`, `tx_out_id`, and `tx_out_index`. This is produced by `tx_out.value = "enable"`, or by `tx_out.force_tx_in = true` with `tx_out.value = "consumed"`. |
| `consumed` | A complete `tx_out.consumed_by_tx_id` history. This is produced by `tx_out.value = "consumed"`. |
| `auto` | Uses `tx_in` when it has rows, otherwise uses `tx_out.consumed_by_tx_id` when at least one output records a consuming transaction. If both representations are structurally present but empty, startup fails as ambiguous and asks for an explicit mode. This does not prove that the selected representation has complete history. |

`db_sync_address_mode` maps to `insert_options.tx_out.use_address_table`:

| Midnight mode | db-sync setting | Required columns |
|---------------|-----------------|------------------|
| `inline` | `false` | `tx_out.address` |
| `address_table` | `true` | `tx_out.address_id`, plus `address.id` and `address.address` |

Both address layouts are supported. The transaction-output modes `prune`, `bootstrap`, and
`disable` are not supported because they do not retain the complete output history required by
Midnight queries. `consumed` with `force_tx_in = false` is supported with Midnight's `consumed`
mode; `consumed` with `force_tx_in = true` can use either complete representation.

The other db-sync data used by the main-chain follower must also be retained. In configurations
that expose the individual switches, keep ledger-derived data, multi-assets, Plutus/datum data,
and the C-to-M bridge metadata enabled. In current db-sync configuration terms, this means:

- `insert_options.ledger = "enable"`
- `insert_options.multi_asset.enable = true`
- `insert_options.plutus.enable = true`
- `insert_options.metadata.enable = true`
- if `insert_options.metadata.keys` filters retained metadata, it includes key `6500973`
- datum and metadata JSON data remains present. Both values of
  `insert_options.remove_jsonb_from_schema` are supported; Midnight casts retained text values to
  `jsonb` while reading them.

The C-to-M bridge reads `tx_metadata` rows with key `6500973`. The current follower does not query
db-sync governance tables, so `insert_options.governance` is not a compatibility requirement for
this version.

The db-sync `only_utxo`, `only_governance`, and `disable_all` presets are therefore not suitable
for a full Midnight node. See the upstream
[cardano-db-sync configuration reference](https://github.com/IntersectMBO/cardano-db-sync/blob/master/doc/configuration.md)
for the behavior of these settings.

#### Historical completeness

Column presence is not proof of data completeness. In particular, changing a running db-sync
instance from `tx_out.value = "enable"` to `"consumed"`, or enabling `force_tx_in` after part of
the chain has already been synced, does not by itself guarantee that old spends have been
backfilled. A pruned or bootstrapped database is also insufficient even when its current UTXO set
is complete.

Before selecting a mode, complete the cardano-db-sync migration or backfill procedure for the
entire block and epoch range Midnight will query, or restore/resync from a compatible full-history
snapshot. Validate the historical data separately. `auto` only detects schema shape and whether
either input representation contains evidence; `verify` only checks schema shape and indexes. Neither mode audits
historical completeness. Schema checks also cannot determine whether `tx_metadata` key `6500973`
was filtered or whether its older rows are missing. Enabling the metadata key for new blocks does
not backfill bridge metadata history.

### Schema-management modes

| Mode | Behavior | Database privileges |
|------|----------|---------------------|
| `apply` | Creates missing indexes for the current command with `CREATE INDEX CONCURRENTLY`. cNight genesis commands also set recommended per-table autovacuum reloptions. This is the backward-compatible default. | Ownership of the affected tables, or an equivalent administrative role, in addition to read access. |
| `verify` | Performs read-only layout and index checks for the current command. A missing required index fails initialization. cNight genesis commands also warn about non-recommended autovacuum settings. | `CONNECT`, schema `USAGE`, table `SELECT`, and access to PostgreSQL catalog metadata. |
| `skip` | Resolves and validates the selected layout but does not create, alter, or verify indexes and autovacuum settings. | Read access only, but the operator assumes responsibility for correctness and performance. |

The managed manifest depends on the entry point:

- Normal node startup and `generate-permissioned-candidates-genesis` apply or verify the
  runtime/candidate manifest. It includes the selected transaction-input and address indexes.
- `generate-c-night-genesis` applies or verifies the broader cNight genesis manifest and its
  autovacuum recommendations. The cNight phase of `generate-genesis-config` does the same, and its
  permissioned-candidates phase also manages the runtime/candidate manifest.
- Standalone genesis commands that only resolve the query layout, such as ICS or reserve genesis,
  do not manage an index manifest.

Normal node startup does not enforce every cNight genesis index or autovacuum recommendation.
Install the combined manifest below before using a read-only role for both genesis generation and
normal operation. `db_sync_schema_mode` controls Midnight-issued `CREATE INDEX` and `ALTER TABLE`
statements; the node never inserts, updates, or deletes Cardano chain data.

#### Index manifests

Verification is based on index structure, not index name. An existing index is accepted when it is
valid, ready, non-partial, uses one of the listed access methods, and has the listed columns as its
leading keys. For example, either `(tx_out_id)` or `(tx_out_id, ident)` satisfies the
`ma_tx_out(tx_out_id)` requirement. This allows operators to retain standard db-sync indexes and
their own index names.

Layout-independent indexes in the combined runtime/candidate and cNight genesis manifests:

| Relation | Access method | Leading keys | Managed by |
|----------|---------------|--------------|------------|
| `multi_asset` | btree | `policy`, `name` | cNight genesis |
| `ma_tx_out` | btree | `ident` | Both |
| `ma_tx_out` | btree | `tx_out_id` | Both |
| `block` | btree | `block_no` | cNight genesis |
| `tx` | btree | `block_id` | cNight genesis |
| `tx_out` | btree | `tx_id` | cNight genesis |

Additional indexes for the selected address layout:

| Address mode | Relation | Access method | Leading keys | Managed by |
|--------------|----------|---------------|--------------|------------|
| `inline` | `tx_out` | hash or btree | `address` | Both |
| `address_table` | `address` | hash or btree | `address` | Both |
| `address_table` | `tx_out` | btree | `address_id` | Both |

Additional indexes for the selected transaction-input layout:

| Transaction-input mode | Relation | Access method | Leading keys | Managed by |
|------------------------|----------|---------------|--------------|------------|
| `tx_in` | `tx_in` | btree | `tx_in_id` | Both |
| `tx_in` | `tx_in` | btree | `tx_out_id`, `tx_out_index` | Both |
| `consumed` | `tx_out` | btree | `consumed_by_tx_id` | Both |

#### Operator-managed SQL

Run index creation as the db-sync table owner or a dedicated migration role, not as the Midnight
read-only role. `CREATE INDEX CONCURRENTLY` must be run outside a transaction. The following names
match the names used by `apply` mode; structurally compatible indexes with other names are also
accepted. Do not blindly execute every statement: a normal db-sync database already contains
several compatible indexes under different names, and PostgreSQL's `IF NOT EXISTS` only compares
the proposed name. Run `verify` first (or inspect the catalog), then create only the structures it
reports as missing.

For the full combined manifest, independent of layout:

```sql
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_multi_asset_policy_name
    ON multi_asset (policy, name);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ma_tx_out_ident
    ON ma_tx_out (ident);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ma_tx_out_id_ident
    ON ma_tx_out (tx_out_id, ident);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_block_block_no
    ON block (block_no);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_block_id
    ON tx (block_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_out_tx_id
    ON tx_out (tx_id);
```

The standard db-sync btree on `ma_tx_out(tx_out_id)` also satisfies the second requirement. Apply
mode creates the covering `(tx_out_id, ident)` form only when no tx-out-id-leading index exists.

For `db_sync_address_mode = "inline"`:

```sql
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_out_address
    ON tx_out USING hash (address);
```

For `db_sync_address_mode = "address_table"`:

```sql
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_address_address
    ON address USING hash (address);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_out_address_id
    ON tx_out (address_id);
```

For `db_sync_tx_input_mode = "tx_in"`:

```sql
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_in_tx_in_id
    ON tx_in (tx_in_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_in_tx_out_id_tx_out_index
    ON tx_in (tx_out_id, tx_out_index);
```

For `db_sync_tx_input_mode = "consumed"`:

```sql
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tx_out_consumed_by_tx_id
    ON tx_out (consumed_by_tx_id);
```

After applying the manifest, start the node or run the genesis command once with
`db_sync_schema_mode = "verify"`. This catches an invalid or partial index, including a name
collision where `IF NOT EXISTS` left an incompatible index in place.

#### Autovacuum recommendations

When the cNight genesis schema manifest is managed, `apply` sets the following reloptions on the
db-sync tables queried by cNight observation:

```sql
ALTER TABLE <table> SET (
    autovacuum_analyze_scale_factor = 0.01,
    autovacuum_vacuum_scale_factor = 0.05
);
```

The base table set is `block`, `tx`, `tx_out`, `ma_tx_out`, and `datum`. It also includes `tx_in`
for the `tx_in` layout and `address` for the `address_table` layout. A DBA can apply equivalent
per-table or cluster-level tuning. `verify` warns about missing table reloptions but does not fail,
because suitable cluster-level settings cannot be inferred from relation metadata alone.

### Read-only deployment workflow

For an existing db-sync database managed separately from Midnight:

1. Confirm the db-sync layout and verify that the selected transaction-input representation has
   complete history for the range Midnight will query.
2. Grant a separate Midnight login `CONNECT` on the database, `USAGE` on the db-sync schema, and
   `SELECT` on the db-sync tables. Do not grant table ownership, `CREATE`, `INSERT`, `UPDATE`, or
   `DELETE`.
3. Configure explicit layout modes and set `db_sync_schema_mode = "verify"`. Run the normal node
   and each manifest-managing genesis entry point you plan to use to identify only the missing
   index structures.
4. As the db-sync owner or migration role, create those missing indexes. Apply the recommended
   per-table autovacuum settings, or document equivalent cluster-level tuning; a mismatch is a
   performance warning rather than a `verify` failure.
5. Rerun `verify`. Use `SHOW_CONFIG=1` to confirm the effective values before the final genesis and
   node runs. Each entry point fails initialization when an index required by its managed manifest
   is absent or unusable.

For a db-sync database configured with `tx_out.value = "consumed"`,
`tx_out.force_tx_in = false`, and `tx_out.use_address_table = true`, the exact read-only profile is:

```toml
db_sync_tx_input_mode = "consumed"
db_sync_address_mode = "address_table"
db_sync_schema_mode = "verify"
```

The equivalent environment variables are:

```sh
export DB_SYNC_TX_INPUT_MODE=consumed
export DB_SYNC_ADDRESS_MODE=address_table
export DB_SYNC_SCHEMA_MODE=verify
```

The connection role can be made read-only at the PostgreSQL level as an additional guardrail. For
example, run the grants as an administrator, substituting the actual database, schema, and role:

```sql
GRANT CONNECT ON DATABASE cexplorer TO midnight_reader;
GRANT USAGE ON SCHEMA public TO midnight_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO midnight_reader;
ALTER ROLE midnight_reader SET default_transaction_read_only = on;
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

The `pc-chain-config.json` is an output of the PartnerChains chain initialisation. See the [Partner Chains Chain Builder Documentation](https://github.com/midnightntwrk/partner-chains/blob/898ee1cb082dd1002afdd8bcf01b4aee494c03f3/docs/user-guides/chain-builder.md#storing-the-main-chain-configuration) for more information on this.

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
