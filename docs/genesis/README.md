# Genesis Documentation

This directory contains documentation for Midnight network genesis generation and verification.

## Contents

| Document | Description |
|----------|-------------|
| [Generation Guide](generation.md) | How to generate genesis configuration and chain specifications |
| [Verification Guide](verification.md) | How to verify generated chain specifications |

## Quick Start

### Genesis Generation

Generate a new chain specification for a network:

```bash
./scripts/genesis/genesis-generation.sh
```

See [Generation Guide](generation.md) for detailed instructions.

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
│                         GENERATION PHASE                                  │
│                     (genesis-generation.sh)                               │
│                                                                           │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                  │
│   │   Step 1    │───▶│   Step 2    │───▶│   Step 3    │                  │
│   │   Config    │    │   Ledger    │    │   Chain     │                  │
│   │   Files     │    │   State     │    │   Spec      │                  │
│   └─────────────┘    └─────────────┘    └─────────────┘                  │
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
                                    │
                                    ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                                                                           │
│                        VERIFICATION PHASE                                 │
│                    (genesis-verification.sh)                              │
│                                                                           │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌───────────┐ │
│   │   Step 0    │───▶│   Step 1    │───▶│   Step 2    │───▶│  Step 3   │ │
│   │   Cardano   │    │   Config    │    │   Ledger    │    │  Dparam   │ │
│   │   Tip       │    │   Regen     │    │   State     │    │  Check    │ │
│   └─────────────┘    └─────────────┘    └─────────────┘    └───────────┘ │
│                                                                  │        │
│                                                                  ▼        │
│                                                            ┌───────────┐ │
│                                                            │  Step 4   │ │
│                                                            │  Auth     │ │
│                                                            │  Script   │ │
│                                                            └───────────┘ │
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

Available networks for genesis generation/verification:
- `mainnet` - Production network
- `qanet` - QA testing network
- `devnet` - Development network
- `govnet` - Governance testing network
- `node-dev-01` - Single node development
- `preview` - Preview/staging network
- `preprod` - Pre-production network

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
   - Local: `postgres://cardano@localhost:54322/cexplorer`
   - Set `DB_SYNC_POSTGRES_CONNECTION_STRING` environment variable

3. **For generation**: `GITHUB_TOKEN` for Earthly targets

4. **For verification**: Generated chain specification files

## Related Documentation

- [AGENTS.md](../../AGENTS.md) - Build commands and project overview
- [Earthfile](../../Earthfile) - Earthly targets for genesis generation
