# Genesis Verification Guide

This document describes the genesis verification process for Midnight networks. Verification ensures that the generated chain specification is correct and matches the expected Cardano smart contract state.

## Overview

Genesis verification validates the chain specification before network launch. The process involves five verification steps:

0. **Cardano Tip Finalization** - Verifies the Cardano block has enough confirmations
1. **Config File Regeneration** - Regenerates config files and compares with existing
2. **LedgerState Verification** - Validates genesis state contents from chain-spec-raw.json
3. **Dparameter Verification** - Checks system parameters consistency
4. **Auth Script Verification** - Verifies upgradable contracts share the same authorization script

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
│ ledger-parameters-  │────────────────┘                    │     = 24B invariant │
│ config.json         │                                     ├─────────────────────┤
└─────────────────────┘                                     │ 2d. LedgerParameters│
                                                            │     match config    │
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
│ ics-addresses.json  │───────▶│ (runs all 3 verify   │     │ 4b. two_stage_policy│
├─────────────────────┤        │ commands internally) │     │     embedded in code│
│ permissioned-       │───────▶│                      │     ├─────────────────────┤
│ candidates-         │        └──────────────────────┘     │ 4c. observed auth   │
│ addresses.json      │                │                    │     matches expected│
└─────────────────────┘                │                    ├─────────────────────┤
                                       ▼                    │ 4d. all contracts   │
┌─────────────────────┐        ┌──────────────────────┐     │     share same auth │
│ Cardano db-sync     │───────▶│ Query observed       │     └─────────────────────┘
│ (PostgreSQL)        │        │ auth scripts         │
└─────────────────────┘        └──────────────────────┘
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
    --network <network_name>
```

Outputs status markers:
- `DUST_STATE_OK` - DustState matches cnight-config.json
- `EMPTY_STATE_OK` - State is empty (mainnet only)
- `SUPPLY_INVARIANT_OK` - Total NIGHT supply equals 24B
- `LEDGER_PARAMETERS_OK` - LedgerParameters match config

### Auth Script Verification

Verifies all upgradable contracts use the expected authorization script:

```bash
# Verify all contracts at once
midnight-node verify-auth-script --cardano-tip <block_hash>

# Verify individual contracts
midnight-node verify-federated-authority-auth-script --cardano-tip <block_hash>
midnight-node verify-ics-auth-script --cardano-tip <block_hash>
midnight-node verify-permissioned-candidates-auth-script --cardano-tip <block_hash>
```

For each contract, verification checks:
1. The `compiled_code` hash matches the `policy_id` (Plutus V3: `blake2b_224(0x03 || script_bytes)`)
2. The `two_stage_policy_id` is embedded in the `compiled_code`
3. The authorization script observed on Cardano matches the expected value from config

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
| `cardano-tip.json` | All steps | Cardano block hash reference point |
| `pc-chain-config.json` | Step 0 | Contains `security_parameter` for finalization check |
| `chain-spec-raw.json` | Step 2 | Raw chain specification with genesis state |
| `cnight-config.json` | Steps 1, 2 | cNIGHT genesis configuration |
| `ics-config.json` | Step 1 | ICS genesis configuration |
| `federated-authority-config.json` | Step 1 | Federated authority configuration |
| `permissioned-candidates-config.json` | Steps 1, 3 | Permissioned candidates configuration |
| `system-parameters-config.json` | Step 3 | System parameters including Dparameter |
| `ledger-parameters-config.json` | Step 2 | Ledger parameters |

### Address Files (for Regeneration)

| File | Used By | Description |
|------|---------|-------------|
| `cnight-addresses.json` | Step 1 | cNIGHT contract addresses |
| `ics-addresses.json` | Steps 1, 4 | ICS contract addresses with compiled code |
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
   - Local: `postgres://cardano@localhost:54322/cexplorer`
   - Or a remote db-sync instance

3. **Generated chain specification** and config files for the network

### Running the Tool

```bash
./scripts/genesis/genesis-verification.sh
```

### Step-by-Step Verification

#### 1. Select Network

Choose from available networks:
- `mainnet`
- `qanet`
- `devnet`
- `govnet`
- `node-dev-01`
- `preview`
- `preprod`

#### 2. Provide Configuration

Enter when prompted:

1. **DB Sync PostgreSQL connection string**
   - Default: `postgres://cardano@localhost:54322/cexplorer`

2. **Cardano block hash (tip)**
   - If `cardano-tip.json` exists, the value is prefilled
   - Must be a finalized block for reliable verification

#### 3. Run Verification Steps

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
- Validates DustState, supply invariant, and parameters

**Step 3: Dparameter Verification**
- Checks system-parameters-config.json consistency
- Verifies num_registered_candidates = 0
- Verifies num_permissioned_candidates matches actual count

**Step 4: Auth Script Verification**
- Verifies all upgradable contracts
- Checks compiled code hashes
- Confirms authorization scripts match

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
     c. Total NIGHT supply invariance (24B)
     d. LedgerParameters match config
  3. Dparameter Verification - Verifies system-parameters-config.json consistency
  4. Auth Script Verification - Verifies upgradable contracts share the same auth script

>>> Select Network

Available networks:

1) mainnet
2) qanet
...

Select network (1-7): 2
[PASS] Selected network: qanet

>>> Configuration

[INFO] Found cardano tip in res/qanet/cardano-tip.json
Cardano block hash (tip) [0x6b0eda47...]:

Configuration Summary:
  Network:              qanet
  Security Parameter:   432
  Cardano Tip:          0x6b0eda47...

>>> Step 0: Verify Cardano Tip is Finalized

Block 12345678 has 500 confirmations (required: 432)
[PASS] Step 0: Cardano tip is finalized!

>>> Step 1: Regenerate and Compare Genesis Config Files

  > Regenerating cnight-config.json...
[PASS] cnight-config.json matches
  > Regenerating ics-config.json...
[PASS] ics-config.json matches
...

=================================================================
  Verification Summary
=================================================================

Results for qanet:

  [PASS] Step 0: Cardano Tip Finalization
  [PASS] Step 1: Config File Regeneration
  [PASS] Step 2: LedgerState Verification
  [PASS] Step 3: Dparameter Verification
  [PASS] Step 4: Auth Script Verification

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
