# Genesis Generation Guide

This document describes the genesis generation process for Midnight networks, including the required input files, commands, and outputs.

## Overview

Genesis generation creates the initial chain state for a Midnight network. The process involves three main steps:

1. **Genesis Config Generation** - Queries Cardano smart contracts to generate config files from address files
2. **Ledger State Generation** - Creates the initial ledger state files (`genesis_block_*.mn`, `genesis_state_*.mn`)
3. **Chain Spec Generation** - Combines all inputs to produce the final chain specification

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Genesis Generation Flow                             │
└─────────────────────────────────────────────────────────────────────────────┘

 Step 1: Generate Config Files from Addresses
 ─────────────────────────────────────────────

┌─────────────────────┐        ┌──────────────────────┐     ┌─────────────────────┐
│ cnight-addresses.   │───────▶│ midnight-node        │────▶│ cnight-config.json  │──┐
│ json                │        │ generate-c-night-    │     └─────────────────────┘  │
└─────────────────────┘        │ genesis              │                              │
                               └──────────────────────┘                              │
┌─────────────────────┐        ┌──────────────────────┐     ┌─────────────────────┐  │
│ ics-addresses.json  │───────▶│ midnight-node        │────▶│ ics-config.json     │──┤
└─────────────────────┘        │ generate-ics-genesis │     └─────────────────────┘  │
                               └──────────────────────┘                              │
┌─────────────────────┐        ┌──────────────────────┐     ┌─────────────────────┐  │
│ federated-authority-│───────▶│ midnight-node        │────▶│ federated-authority-│──┤
│ addresses.json      │        │ generate-federated-  │     │ config.json         │  │
└─────────────────────┘        │ authority-genesis    │     └─────────────────────┘  │
                               └──────────────────────┘                              │
┌─────────────────────┐        ┌──────────────────────┐     ┌─────────────────────┐  │
│ permissioned-       │───────▶│ midnight-node        │────▶│ permissioned-       │──┤
│ candidates-         │        │ generate-permissioned│     │ candidates-         │  │
│ addresses.json      │        │ -candidates-genesis  │     │ config.json         │  │
└─────────────────────┘        └──────────────────────┘     └─────────────────────┘  │
                                                                                     │
                                                                                     │
 Step 2: Generate Ledger State                                                       │
 ─────────────────────────────                                                       │
                                                                                     │
┌─────────────────────┐                                                              │
│ ledger-parameters-  │──┐                                                           │
│ config.json         │  │                                                           │
└─────────────────────┘  │     ┌──────────────────────┐     ┌─────────────────────┐  │
                         ├────▶│ earthly +rebuild-    │────▶│ genesis_block_*.mn  │  │
┌─────────────────────┐  │     │ genesis-state-*      │     │ genesis_state_*.mn  │  │
│ cnight-config.json  │──┤     └──────────────────────┘     └─────────────────────┘  │
├─────────────────────┤  │                                           │               │
│ ics-config.json     │──┘                                           │               │
└─────────────────────┘                                              │               │
        ▲                                                            │               │
        │ (generated in Step 1)                                      │               │
        └────────────────────────────────────────────────────────────│───────────────┘
                                                                     │
                                                                     │
 Step 3: Generate Chain Specification                                │
 ────────────────────────────────────                                │
                                                                     │
┌─────────────────────┐                                              │
│ pc-chain-config.json│──┐                                           │
├─────────────────────┤  │                                           │
│ system-parameters-  │──┤                                           │
│ config.json         │  │                                           │
├─────────────────────┤  │     ┌──────────────────────┐              │
│ registered-         │──┤     │                      │              │
│ candidates-         │  │     │ earthly +rebuild-    │◀─────────────┘
│ addresses.json      │  ├────▶│ chainspec            │
├─────────────────────┤  │     │ --NETWORK=<network>  │     ┌─────────────────────┐
│ cnight-config.json  │──┤     │                      │────▶│ chain-spec.json     │
├─────────────────────┤  │     │                      │     │ chain-spec-raw.json │
│ ics-config.json     │──┤     └──────────────────────┘     │ chain-spec-abridged │
├─────────────────────┤  │                                  │ .json               │
│ federated-authority-│──┤                                  └─────────────────────┘
│ config.json         │  │
├─────────────────────┤  │
│ permissioned-       │──┘
│ candidates-         │
│ config.json         │
└─────────────────────┘
```

## Input Files

All input files are located in `res/<network>/` directory.

### Address Files (Manual Configuration)

These files contain Cardano smart contract addresses and must be configured before genesis generation:

| File | Description |
|------|-------------|
| `cnight-addresses.json` | cNIGHT mapping validator address and token policy |
| `ics-addresses.json` | Illiquid Circulation Supply validator address for treasury funding |
| `federated-authority-addresses.json` | Federated authority governance contract addresses |
| `permissioned-candidates-addresses.json` | Permissioned candidates policy ID |
| `registered-candidates-addresses.json` | Initial registered block producer candidates |

### Configuration Files

| File | Description |
|------|-------------|
| `ledger-parameters-config.json` | Ledger parameters (epoch length, slot duration, etc.) |
| `pc-chain-config.json` | Partner chain configuration (security parameter, etc.) |
| `system-parameters-config.json` | System-level parameters |

### Generated Config Files (from Address Files)

These files are generated by querying Cardano smart contracts using the address files above. They must be generated **before** running ledger state generation:

| File | Generated From | Description |
|------|----------------|-------------|
| `cnight-config.json` | `cnight-addresses.json` | cNIGHT observation genesis (DUST address registrations) |
| `ics-config.json` | `ics-addresses.json` | ICS genesis (treasury funding from locked cNIGHT) |
| `federated-authority-config.json` | `federated-authority-addresses.json` | Initial governance authority members |
| `permissioned-candidates-config.json` | `permissioned-candidates-addresses.json` | Initial permissioned candidates |

## Output Files

### Ledger State Files

Located in `res/genesis/`:

| File | Description |
|------|-------------|
| `genesis_block_<network>.mn` | Initial block data |
| `genesis_state_<network>.mn` | Initial ledger state |

### Chain Specification Files

Located in `res/<network>/`:

| File | Description |
|------|-------------|
| `chain-spec.json` | Human-readable chain specification |
| `chain-spec-raw.json` | Raw chain specification (used by nodes) |
| `chain-spec-abridged.json` | Abridged version for reference |

## Commands

### Individual Genesis Commands

The `midnight-node` binary provides several commands for genesis generation:

```bash
# Generate all genesis configs at once
midnight-node generate-genesis-config --cardano-tip <block_hash>

# Generate individual configs
midnight-node generate-c-night-genesis --cardano-tip <block_hash>
midnight-node generate-ics-genesis --cardano-tip <block_hash>
midnight-node generate-federated-authority-genesis --cardano-tip <block_hash>
midnight-node generate-permissioned-candidates-genesis --cardano-tip <block_hash>
```

### Earthly Targets

```bash
# Generate ledger state for a specific network
earthly --secret GITHUB_TOKEN -P +rebuild-genesis-state-<network> --RNG_SEED=<seed>

# Generate chain specification
earthly -P +rebuild-chainspec --NETWORK=<network>

# Rebuild all chain specs
earthly -P +rebuild-all-chainspecs
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `CFG_PRESET` | Network preset (e.g., `qanet`, `preview`, `devnet`) |
| `DB_SYNC_POSTGRES_CONNECTION_STRING` | PostgreSQL connection to Cardano db-sync |
| `CARDANO_SECURITY_PARAMETER` | Cardano security parameter (default from pc-chain-config.json) |
| `ALLOW_NON_SSL` | Allow non-SSL database connections (dev only) |
| `GITHUB_TOKEN` | GitHub token for Earthly private resources |

## Dependency Sequence

The genesis generation process has strict dependencies:

1. **Address files** must exist before generating config files
2. **Config files** (`cnight-config.json`, `ics-config.json`) must be generated before ledger state generation
3. **Ledger state files** and **all config files** must exist before chain spec generation

```
Address Files ──▶ Config Files ──▶ Ledger State ──▶ Chain Spec
     │                  │                │               │
     │                  │                │               │
     ▼                  ▼                ▼               ▼
  Manual           generate-*      +rebuild-       +rebuild-
  config           commands        genesis-state   chainspec
```

---

## Interactive Genesis Generation Tool

For a guided experience, use the interactive shell script:

```bash
./scripts/genesis/genesis-generation.sh
```

See [Step-by-Step Guide](#step-by-step-guide-using-the-interactive-tool) below.

---

## Step-by-Step Guide Using the Interactive Tool

The `genesis-generation.sh` script provides an interactive wizard for genesis generation.

### Prerequisites

1. **Build the midnight-node binary** (release mode):
   ```bash
   cargo build --release -p midnight-node
   ```

2. **Access to Cardano db-sync database**:
   - Local: `postgres://cardano@localhost:54322/cexplorer`
   - Or a remote db-sync instance

3. **Cardano block hash** (tip) for querying smart contract state

4. **GITHUB_TOKEN** environment variable (for Earthly targets)

### Running the Tool

```bash
./scripts/genesis/genesis-generation.sh
```

### Step 1: Select Network

The tool presents available networks:
- `qanet`
- `devnet`
- `govnet`
- `node-dev-01`
- `preview`

### Step 2: Provide Configuration

Enter the following when prompted:

1. **DB Sync PostgreSQL connection string**
   - Default: `postgres://cardano@localhost:54322/cexplorer`
   - Edit or press Enter to accept default

2. **Cardano block hash (tip)**
   - Required - the Cardano block hash to use as reference point
   - Example: `0x1234abcd...` (64 hex characters)

3. **RNG seed for ledger state**
   - Default: `0000000000000000000000000000000000000000000000000000000000000037`
   - Used for deterministic genesis generation

### Step 3: Ledger State Generation

Before generating the ledger state, config files must exist:

1. The tool checks if `cnight-config.json` and `ics-config.json` exist
2. If missing or if you choose to regenerate, it runs:
   - `midnight-node generate-c-night-genesis`
   - `midnight-node generate-ics-genesis`
3. Then runs: `earthly +rebuild-genesis-state-<network>`

**Output files:**
- `res/genesis/genesis_block_<network>.mn`
- `res/genesis/genesis_state_<network>.mn`

### Step 4: Genesis Config Generation

Generates configuration files from Cardano smart contract state:

```bash
midnight-node generate-genesis-config --cardano-tip <block_hash>
```

**Output files:**
- `res/<network>/cnight-config.json`
- `res/<network>/ics-config.json`
- `res/<network>/federated-authority-config.json`
- `res/<network>/permissioned-candidates-config.json`

If configs were already generated in Step 3, the tool offers to keep them and only generate the remaining files.

### Step 5: Chain Spec Generation

Creates the final chain specification:

```bash
earthly -P +rebuild-chainspec --NETWORK=<network>
```

**Output files:**
- `res/<network>/chain-spec.json`
- `res/<network>/chain-spec-raw.json`
- `res/<network>/chain-spec-abridged.json`

### Example Session

```
═══════════════════════════════════════════════════════════════
  Midnight Genesis Generation Tool
═══════════════════════════════════════════════════════════════

This tool will guide you through the chain specification generation process.
It consists of three main steps:

  1. Ledger State Generation - Creates initial ledger state (genesis_block, genesis_state)
  2. Genesis Config Generation - Generates config files from smart contract addresses
  3. Chain Spec Generation - Creates the final chain specification files

▶ Select Network

Available networks:

1) qanet
2) devnet
3) govnet
4) node-dev-01
5) preview

Select network (1-5): 1
✓ Selected network: qanet

▶ Configuration

DB Sync PostgreSQL connection string [postgres://cardano@localhost:54322/cexplorer]:
Cardano block hash (tip): 0xabc123...
RNG seed for ledger state [0000000000000000000000000000000000000000000000000000000000000037]:

Configuration Summary:
  Network:              qanet
  Security Parameter:   432
  Cardano Tip:          0xabc123...
  RNG Seed:             0000000000000000000000000000000000000000000000000000000000000037

▶ Step 1: Ledger State Generation
...
```

## Troubleshooting

### Database Connection Issues

If you see SSL-related errors:
```bash
export ALLOW_NON_SSL=true  # Only for local development!
```

### Missing GITHUB_TOKEN

```bash
export GITHUB_TOKEN=<your_token>
# Or use direnv with .envrc
```

### Earthly Build Failures

Ensure you have:
- Earthly installed and running
- Docker daemon running
- Sufficient disk space

### Invalid Cardano Tip

Ensure the block hash:
- Is a valid 64-character hex string
- Exists in the db-sync database
- Is recent enough to contain the smart contract data
