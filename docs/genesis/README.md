# Genesis Documentation

This directory contains documentation for Midnight network genesis generation and verification.

## Contents

| Document | Description |
|----------|-------------|
| [Construction Guide](construction.md) | How to generate genesis configuration and chain specifications |
| [Verification Guide](verification.md) | How to verify generated chain specifications |

## Quick Start

### Genesis Construction

Generate a new chain specification for a network:

```bash
./scripts/genesis/genesis-construction.sh
```

See [Construction Guide](construction.md) for detailed instructions.

### Genesis Verification

Verify an existing chain specification:

```bash
./scripts/genesis/genesis-verification.sh
```

See [Verification Guide](verification.md) for detailed instructions.

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Genesis Workflow                               │
└─────────────────────────────────────────────────────────────────────────────┘

                    ┌─────────────────────────────────┐
                    │        Address Files            │
                    │  (manual configuration)         │
                    │                                 │
                    │  - cnight-addresses.json        │
                    │  - ics-addresses.json           │
                    │  - federated-authority-         │
                    │    addresses.json               │
                    │  - permissioned-candidates-     │
                    │    addresses.json               │
                    └─────────────────────────────────┘
                                    │
                                    ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                                                                           │
│                         CONSTRUCTION PHASE                                │
│                     (genesis-construction.sh)                             │
│                                                                           │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                   │
│   │   Step 1    │───▶│   Step 2    │───▶│   Step 3    │                   │
│   │   Config    │    │   Ledger    │    │   Chain     │                   │
│   │   Files     │    │   State     │    │   Spec      │                   │
│   └─────────────┘    └─────────────┘    └─────────────┘                   │
│                                                                           │
└───────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
                    ┌─────────────────────────────────┐
                    │        Output Files             │
                    │                                 │
                    │  - chain-spec.json              │
                    │  - chain-spec-raw.json          │
                    │  - chain-spec-abridged.json     │
                    │  - genesis_block_*.mn           │
                    │  - genesis_state_*.mn           │
                    └─────────────────────────────────┘
                    ┌─────────────────────────────────┐
                    │  Bootnodes                      │
                    │                                 │
                    │  - bootnodes-config.json        │
                    │    Injected into chain-spec     │
                    │    after generation             │
                    └─────────────────────────────────┘
                                    │
                                    ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                                                                           │
│                        VERIFICATION PHASE                                 │
│                    (genesis-verification.sh)                              │
│                                                                           │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌───────────┐  │
│   │   Step 0    │───▶│   Step 1    │───▶│   Step 2    │───▶│  Step 3   │  │
│   │   Cardano   │    │   Config    │    │   Ledger    │    │  Dparam   │  │
│   │   Tip       │    │   Regen     │    │   State     │    │  Check    │  │
│   └─────────────┘    └─────────────┘    └─────────────┘    └───────────┘  │
│                                                                  │        │
│                                                                  ▼        │
│   ┌─────────────┐    ┌─────────────┐                       ┌───────────┐  │
│   │   Step 6    │◀───│   Step 5    │◀──────────────────────│  Step 4   │  │
│   │  Timestamp  │    │   Message   │                       │  Auth     │  │
│   │  Verify     │    │   Verify    │                       │  Script   │  │
│   └─────────────┘    └─────────────┘                       └───────────┘  │
│                                                                           │
└───────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
                    ┌─────────────────────────────────┐
                    │      Verification Result        │
                    │                                 │
                    │  [PASS] All checks passed       │
                    │     - or -                      │
                    │  [FAIL] Some checks failed      │
                    └─────────────────────────────────┘
```

## Key Concepts

### Networks

Available networks for genesis generation:
- `mainnet` - Production network
- `qanet` - QA testing network
- `devnet` - Development network
- `govnet` - Governance testing network
- `preview` - Preview/staging network
- `preprod` - Pre-production network

The genesis verification script (`genesis-verification.sh`) targets `mainnet` only.

### Cardano Tip

The Cardano block hash (`cardano_tip`) serves as a reference point for querying smart contract state. It should be:
- A finalized block (enough confirmations based on `security_parameter`)
- Recent enough to contain all deployed smart contract data

The `cardano-tip.json` file in each network's `res/<network>/` directory stores this value.

### Files Location

| Directory | Contents |
|-----------|----------|
| `res/<network>/` | Network-specific configuration and address files |
| `res/genesis/` | Generated ledger state files (`genesis_block_*.mn`, `genesis_state_*.mn`) |
| `scripts/genesis/` | Interactive generation and verification scripts |

## Prerequisites

1. **midnight-node binary** (release mode):
   ```bash
   cargo build --release -p midnight-node
   ```

2. **Cardano db-sync access**:
   - Local: `postgres://postgres:postgres@localhost:5432/cexplorer`
   - Set `DB_SYNC_POSTGRES_CONNECTION_STRING` environment variable

3. **For verification**: Generated chain specification files

## Related Documentation

- [AGENTS.md](../../AGENTS.md) - Build commands and project overview
- [Earthfile](../../Earthfile) - Earthly targets for genesis generation
