# Genesis Verification Guide

This document describes the genesis verification process for Midnight networks. Verification ensures that the generated chain specification is correct and matches the expected Cardano smart contract state.

## Overview

Genesis verification validates the chain specification before network launch. The process involves seven verification steps:

0. **Cardano Tip Finalization** - Verifies the Cardano block has enough confirmations
1. **Config File Regeneration** - Regenerates config files and compares with existing
2. **LedgerState Verification** - Validates genesis state contents from chain-spec-raw.json
3. **Dparameter Verification** - Checks system parameters consistency
4. **Auth Script Verification** - Verifies all upgradable contracts share the same authorization script
5. **Genesis Message Verification** - Verifies the genesis remark message matches message-config.json
6. **Genesis Timestamp Verification** - Verifies the genesis timestamp matches cardano-tip.json

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Genesis Verification Flow                           │
└─────────────────────────────────────────────────────────────────────────────┘

 Step 0: Cardano Tip Finalization (Mandatory)
 ────────────────────────────────────────────

┌─────────────────────┐        ┌──────────────────────┐     ┌─────────────────────┐
│ cardano-tip.json    │───────▶│ midnight-node        │────▶│ PASS: Block has     │
│ (block hash)        │        │ verify-cardano-tip-  │     │ enough confirmations│
└─────────────────────┘        │ finalized            │     └─────────────────────┘
                               └──────────────────────┘
┌─────────────────────┐                │
│ pc-chain-config.json│────────────────┘
│ (security_parameter)│
└─────────────────────┘


 Step 1: Config File Regeneration
 ────────────────────────────────

┌─────────────────────┐        ┌──────────────────────┐     ┌─────────────────────┐
│ *-addresses.json    │───────▶│ midnight-node        │────▶│ Regenerated         │
│ (all address files) │        │ generate-*-genesis   │     │ *-config.json       │
└─────────────────────┘        └──────────────────────┘     └─────────────────────┘
                                                                     │
                                                                     ▼
┌─────────────────────┐                                     ┌─────────────────────┐
│ Existing            │────────────────────────────────────▶│ Compare JSON        │
│ *-config.json       │                                     │ (diff check)        │
└─────────────────────┘                                     └─────────────────────┘
                                                                     │
                                                                     ▼
                                                            ┌─────────────────────┐
                                                            │ PASS: Files match   │
                                                            │ FAIL: Files differ  │
                                                            └─────────────────────┘


 Step 2: LedgerState Verification
 ────────────────────────────────

┌─────────────────────┐        ┌──────────────────────┐     ┌─────────────────────┐
│ chain-spec-raw.json │───────▶│ midnight-node        │────▶│ 2a. DustState       │
│ (genesis_state)     │        │ verify-ledger-state- │     │     matches config  │
└─────────────────────┘        │ genesis              │     ├─────────────────────┤
                               └──────────────────────┘     │ 2b. Empty state     │
┌─────────────────────┐                │                    │     (mainnet only)  │
│ cnight-config.json  │────────────────┤                    ├─────────────────────┤
├─────────────────────┤                │                    │ 2c. NIGHT supply    │
│ ledger-parameters-  │────────────────┤                    │     = 24B invariant │
│ config.json         │                │                    │     + reserve & ICS │
├─────────────────────┤                │                    ├─────────────────────┤
│ cardano-tip.json    │────────────────┘                    │ 2d. LedgerParameters│
│ (timestamp)         │                                     │     match config    │
└─────────────────────┘                                     ├─────────────────────┤
                                                            │ 2e. Genesis         │
                                                            │     timestamp in    │
                                                            │     state histories │
                                                            └─────────────────────┘


 Step 3: Dparameter Verification
 ───────────────────────────────

┌─────────────────────┐        ┌──────────────────────┐     ┌─────────────────────┐
│ system-parameters-  │───────▶│ JSON comparison      │────▶│ 3a. num_registered  │
│ config.json         │        │                      │     │     _candidates = 0 │
└─────────────────────┘        └──────────────────────┘     ├─────────────────────┤
                                        ▲                   │ 3b. num_permissioned│
┌─────────────────────┐                 │                   │     matches count   │
│ permissioned-       │─────────────────┘                   └─────────────────────┘
│ candidates-         │
│ config.json         │
└─────────────────────┘


 Step 4: Auth Script Verification
 ────────────────────────────────

┌─────────────────────┐        ┌──────────────────────┐     ┌─────────────────────┐
│ federated-authority-│───────▶│ midnight-node        │────▶│ 4a. compiled_code   │
│ addresses.json      │        │ verify-auth-script   │     │     hash = policy_id│
├─────────────────────┤        │                      │     ├─────────────────────┤
│ ics-addresses.json  │───────▶│ (runs all 4 verify   │     │ 4b. two_stage_policy│
├─────────────────────┤        │ commands internally) │     │     embedded in code│
│ permissioned-       │───────▶│                      │     ├─────────────────────┤
│ candidates-         │        │                      │     │ 4c. observed auth   │
│ addresses.json      │        │                      │     │     matches expected│
├─────────────────────┤        └──────────────────────┘     ├─────────────────────┤
│ reserve-addresses.  │───────▶        │                    │ 4d. all contracts   │
│ json                │                │                    │     share same auth │
└─────────────────────┘                ▼                    └─────────────────────┘
┌─────────────────────┐        ┌──────────────────────┐
│ Cardano db-sync     │───────▶│ Query observed       │
│ (PostgreSQL)        │        │ auth scripts         │
└─────────────────────┘        └──────────────────────┘


 Step 5: Genesis Message Verification
 ────────────────────────────────────

┌─────────────────────┐        ┌──────────────────────┐     ┌─────────────────────┐
│ chain-spec-raw.json │───────▶│ midnight-node        │────▶│ 5a. System::remark  │
│ (genesis_extrinsics)│        │ verify-genesis-      │     │     extrinsic found │
└─────────────────────┘        │ message              │     ├─────────────────────┤
                               └──────────────────────┘     │ 5b. Remark matches  │
┌─────────────────────┐                │                    │     message-config   │
│ message-config.json │────────────────┘                    └─────────────────────┘
└─────────────────────┘


 Step 6: Genesis Timestamp Verification
 ──────────────────────────────────────

┌─────────────────────┐        ┌──────────────────────┐     ┌─────────────────────┐
│ chain-spec-raw.json │───────▶│ midnight-node        │────▶│ 6a. Timestamp::set  │
│ (genesis_extrinsics)│        │ verify-genesis-      │     │     extrinsic found │
└─────────────────────┘        │ timestamp            │     ├─────────────────────┤
                               └──────────────────────┘     │ 6b. Timestamp       │
┌─────────────────────┐                │                    │     matches cardano- │
│ cardano-tip.json    │────────────────┘                    │     tip.json         │
│ (timestamp field)   │                                     └─────────────────────┘
└─────────────────────┘
```

## Verification Commands

The `midnight-node` binary provides several verification commands:

### Cardano Tip Finalization

Verifies that a Cardano block hash has enough confirmations based on `security_parameter`:

```bash
midnight-node verify-cardano-tip-finalized --cardano-tip <block_hash>
```

The command checks:
- Block exists in db-sync
- Block has at least `security_parameter` confirmations
- Returns the block number and confirmation count

### LedgerState Verification

Validates the genesis state from `chain-spec-raw.json`:

```bash
midnight-node verify-ledger-state-genesis \
    --chain-spec <path/to/chain-spec-raw.json> \
    --cnight-config <path/to/cnight-config.json> \
    --ledger-parameters-config <path/to/ledger-parameters-config.json> \
    --cardano-tip-config <path/to/cardano-tip.json> \
    --network <network_name>
```

The `--cardano-tip-config` flag is required. It provides the genesis timestamp used for DustState replay and timestamp-in-state verification.

Outputs status markers:
- `DUST_STATE_OK` - DustState matches cnight-config.json
- `EMPTY_STATE_OK` - State is empty (mainnet only)
- `SUPPLY_INVARIANT_OK` - Total NIGHT supply equals 24B, reserve pool and treasury (ICS) match expected values
- `LEDGER_PARAMETERS_OK` - LedgerParameters match config
- `GENESIS_TIMESTAMP_IN_STATE_OK` - Genesis timestamp found in LedgerState root histories (`zswap.past_roots`, `dust.utxo.root_history`, `dust.generation.root_history`)

### Auth Script Verification

Verifies all upgradable contracts use the expected authorization script:

```bash
# Verify all contracts at once
midnight-node verify-auth-script --cardano-tip <block_hash>

# Verify individual contracts
midnight-node verify-federated-authority-auth-script --cardano-tip <block_hash>
midnight-node verify-ics-auth-script --cardano-tip <block_hash>
midnight-node verify-permissioned-candidates-auth-script --cardano-tip <block_hash>
midnight-node verify-reserve-auth-script --cardano-tip <block_hash>
```

For each contract, verification checks:
1. The `compiled_code` hash matches the `policy_id` (Plutus V3: `blake2b_224(0x03 || script_bytes)`)
2. The `two_stage_policy_id` is embedded in the `compiled_code`
3. The authorization script observed on Cardano matches the expected value from config

### Genesis Message Verification

Verifies that the `System::remark` extrinsic in the chain spec matches the expected message from `message-config.json`:

```bash
midnight-node verify-genesis-message \
    --chain-spec <path/to/chain-spec-raw.json> \
    --message-config <path/to/message-config.json>
```

If `--message-config` is not provided, it defaults to `res/<CFG_PRESET>/message-config.json`.

Outputs status markers:
- `GENESIS_MESSAGE_FOUND` - A `System::remark` extrinsic was found in genesis_extrinsics
- `GENESIS_MESSAGE_MATCH` - The remark content matches the expected message

### Genesis Timestamp Verification

Verifies that the `Timestamp::set` extrinsic in the chain spec matches the expected timestamp from `cardano-tip.json`:

```bash
midnight-node verify-genesis-timestamp \
    --chain-spec <path/to/chain-spec-raw.json> \
    --cardano-tip-config <path/to/cardano-tip.json>
```

If `--cardano-tip-config` is not provided, it defaults to `res/<CFG_PRESET>/cardano-tip.json`.

The `cardano-tip.json` `timestamp` field is in **seconds**. The `Timestamp::set` extrinsic stores the timestamp in **milliseconds** (`seconds * 1000`). The verification checks that `extrinsic_timestamp == cardano_tip_timestamp * 1000`.

Outputs status markers:
- `GENESIS_TIMESTAMP_FOUND` - A `Timestamp::set` extrinsic was found in genesis_extrinsics
- `GENESIS_TIMESTAMP_MATCH` - The timestamp matches the expected value

## Environment Variables

| Variable | Description |
|----------|-------------|
| `CFG_PRESET` | Network preset (e.g., `qanet`, `preview`, `devnet`) |
| `DB_SYNC_POSTGRES_CONNECTION_STRING` | PostgreSQL connection to Cardano db-sync |
| `ALLOW_NON_SSL` | Allow non-SSL database connections (dev only) |

## Input Files

### Required for Verification

| File | Used By | Description |
|------|---------|-------------|
| `cardano-tip.json` | Steps 0, 2, 6 | Cardano block hash reference point and genesis timestamp |
| `pc-chain-config.json` | Step 0 | Contains `security_parameter` for finalization check |
| `chain-spec-raw.json` | Steps 2, 5, 6 | Raw chain specification with genesis state and extrinsics |
| `cnight-config.json` | Steps 1, 2 | cNIGHT genesis configuration |
| `ics-config.json` | Step 1 | ICS genesis configuration |
| `reserve-config.json` | Step 1 | Reserve observation genesis configuration |
| `federated-authority-config.json` | Step 1 | Federated authority configuration |
| `permissioned-candidates-config.json` | Steps 1, 3 | Permissioned candidates configuration |
| `system-parameters-config.json` | Step 3 | System parameters including Dparameter |
| `ledger-parameters-config.json` | Step 2 | Ledger parameters |
| `message-config.json` | Step 5 | Genesis remark message |

### Address Files (for Regeneration)

| File | Used By | Description |
|------|---------|-------------|
| `cnight-addresses.json` | Step 1 | cNIGHT contract addresses |
| `ics-addresses.json` | Steps 1, 4 | ICS contract addresses with compiled code |
| `reserve-addresses.json` | Steps 1, 4 | Reserve validator addresses with compiled code |
| `federated-authority-addresses.json` | Steps 1, 4 | Federated authority addresses with compiled code |
| `permissioned-candidates-addresses.json` | Steps 1, 4 | Permissioned candidates addresses with compiled code |

---

## Interactive Verification Tool

For a guided verification experience, use the interactive shell script:

```bash
./scripts/genesis/genesis-verification.sh
```

### Prerequisites

1. **Build the midnight-node binary** (release mode):
   ```bash
   cargo build --release -p midnight-node
   ```

2. **Access to Cardano db-sync database**:
   - Local: `postgres://postgres:postgres@localhost:5432/cexplorer`
   - Or a remote db-sync instance

3. **Generated chain specification** and config files for the network

### Running the Tool

```bash
./scripts/genesis/genesis-verification.sh
```

### Step-by-Step Verification

The script targets the `mainnet` network.

#### 1. Provide Configuration

Enter when prompted:

1. **DB Sync PostgreSQL connection string**
   - Default: `postgres://postgres:postgres@localhost:5432/cexplorer`

2. **Cardano block hash (tip)**
   - If `cardano-tip.json` exists, the value is prefilled
   - Must be a finalized block for reliable verification

#### 2. Run Verification Steps

The tool runs each step sequentially:

**Step 0: Cardano Tip Finalization** (Mandatory)
- Verifies the block has enough confirmations
- If not finalized, prompts to continue or abort

**Step 1: Config File Regeneration**
- Regenerates all config files from addresses
- Compares with existing files
- Reports any differences

**Step 2: LedgerState Verification**
- Extracts genesis state from chain-spec-raw.json
- Validates DustState, supply invariant (including reserve pool and treasury values), parameters, and genesis timestamp in state root histories

**Step 3: Dparameter Verification**
- Checks system-parameters-config.json consistency
- Verifies num_registered_candidates = 0
- Verifies num_permissioned_candidates matches actual count

**Step 4: Auth Script Verification**
- Verifies all upgradable contracts (Federated Authority, ICS, Permissioned Candidates, Reserve)
- Checks compiled code hashes
- Confirms authorization scripts match

**Step 5: Genesis Message Verification**
- SCALE-decodes genesis extrinsics from chain-spec-raw.json
- Finds the `System::remark` extrinsic and compares its content to `message-config.json`

**Step 6: Genesis Timestamp Verification**
- SCALE-decodes genesis extrinsics from chain-spec-raw.json
- Finds the `Timestamp::set` extrinsic and compares its value to `cardano-tip.json`
- Validates unit conversion: `cardano-tip.json` timestamp (seconds) * 1000 = extrinsic timestamp (milliseconds)

### Example Session

```
=================================================================
  Midnight Genesis Verification Tool
=================================================================

This tool verifies the chain specification for a network.
It performs the following checks:

  0. Cardano Tip Finalization - Verifies the Cardano tip has enough confirmations
  1. Config File Regeneration - Regenerates config files and compares with existing
  2. LedgerState Verification - Verifies genesis_state contents from chain-spec-raw.json
     a. DustState matches cnight-config.json system_tx
     b. Empty state for mainnet (no faucet funding)
     c. Total NIGHT supply invariance (24B) + reserve pool and treasury (ICS) values
     d. LedgerParameters match config
     e. Genesis timestamp in state root histories
  3. Dparameter Verification - Verifies system-parameters-config.json consistency
  4. Auth Script Verification - Verifies upgradable contracts share the same auth script
  5. Genesis Message Verification - Verifies genesis remark matches message-config.json
  6. Genesis Timestamp Verification - Verifies genesis timestamp matches cardano-tip.json

[INFO] Network: mainnet

>>> Configuration

[INFO] Found cardano tip in res/mainnet/cardano-tip.json
Cardano block hash (tip) [0x387c6da8...]:

Configuration Summary:
  Network:              mainnet
  Security Parameter:   2160
  Cardano Tip:          0x387c6da8...

>>> Step 0: Verify Cardano Tip is Finalized

Block 12345678 has 500 confirmations (required: 432)
[PASS] Step 0: Cardano tip is finalized!

>>> Step 1: Regenerate and Compare Genesis Config Files

  > Regenerating cnight-config.json...
[PASS] cnight-config.json matches
  > Regenerating ics-config.json...
[PASS] ics-config.json matches
  > Regenerating reserve-config.json...
[PASS] reserve-config.json matches
...

=================================================================
  Verification Summary
=================================================================

Results for mainnet:

  [PASS] Step 0: Cardano Tip Finalization
  [PASS] Step 1: Config File Regeneration
  [PASS] Step 2: LedgerState Verification
  [PASS] Step 3: Dparameter Verification
  [PASS] Step 4: Auth Script Verification
  [PASS] Step 5: Genesis Message Verification
  [PASS] Step 6: Genesis Timestamp Verification

[PASS] All verification checks passed!
```

## Troubleshooting

### Cardano Tip Not Finalized

If Step 0 fails:
- Wait for more Cardano blocks to be produced
- Use a block hash with more confirmations
- Check that db-sync is fully synced

### Config Files Differ

If Step 1 shows differences:
- Review the diff output carefully
- Check if smart contract state changed since generation
- Regenerate config files if needed

### Auth Script Verification Failed

If Step 4 fails:
- Verify the address files contain correct `compiled_code`
- Check that `two_stage_policy_id` matches across contracts
- Ensure db-sync has the transaction data for the contracts

### Database Connection Issues

```bash
export ALLOW_NON_SSL=true  # Only for local development!
```

## Understanding Authorization Scripts

Midnight uses upgradable Cardano smart contracts with a two-stage policy pattern:

1. **Two-Stage Policy** - The governance policy that controls upgrades
2. **Authorization Script** - A Plutus V3 script that references the two-stage policy

For each upgradable contract:
- `policy_id` = `blake2b_224(0x03 || compiled_code)`
- `compiled_code` contains the embedded `two_stage_policy_id`
- All contracts must share the same authorization script

The verification ensures:
- Hashes are computed correctly
- Policy IDs are embedded correctly
- Observed values on Cardano match expected values
