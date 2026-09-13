# Development Workflow

This guide covers best practices and tribal knowledge for working effectively on midnight-node.

## Cargo vs Earthly

**General Rule:** Use cargo commands for iterative development, Earthly for specific tasks only.

### Use Cargo For:
- Day-to-day development
- Iterative compilation during coding
- Running tests
- Code formatting and linting

```bash
cargo check          # Type checking
cargo test           # Run tests
cargo clippy         # Linting
cargo fmt            # Format code
cargo build          # Build debug binary
cargo build --release # Build release binary
```

**Why cargo?** Earthly check will recompile only the code in the repo - all the deps are precompiled so it's not very slow, just slower. Cargo's incremental compilation is much faster for iterative development.

### Use Earthly For:
- Building Docker images
- Generating metadata
- Rebuilding genesis
- Running CI-equivalent checks locally
- Tasks requiring containerized environments

```bash
earthly -P +rebuild-metadata   # Update runtime metadata
earthly -P +rebuild-genesis    # Regenerate genesis state
earthly +build                 # Build in containerized environment
earthly +node-image            # Build node Docker image
earthly doc                    # List all available targets
```

**Why Earthly?** Ensures reproducible builds in clean containerized environments, matches CI behavior exactly.

## Common Development Tasks

### Starting Development

**Step 1: Install Prerequisites**

Choose one:

```bash
# Option A: Manual installation
# Install rustup and earthly following docs/rust-setup.md

# Option B: Use Nix (provides everything automatically)
nix develop
```

**Step 2: Load Environment Variables**

Choose one:

```bash
# Option A: Using direnv (automatic)
cd /path/to/midnight-node  # direnv loads .envrc automatically

# Option B: Manual
source .envrc
```

Note: If you chose Nix in Step 1, you still need to choose an environment loading method from Step 2.

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run integration tests only
cargo test --test integration_test_name
```

### Code Quality Checks

```bash
# Before committing, run these checks
cargo clippy         # Lints and warnings (includes type checking)
cargo fmt            # Format code
cargo test           # Run tests
```

## Ledger Upgrades

When upgrading the midnight-ledger dependency:

### Step 1: Update Dependencies

```bash
# Edit Cargo.toml files with new ledger version
# Then check for compilation errors and update lock file
cargo check
```

### Step 2: Fix Compilation Errors

Common issues during ledger upgrades:
- `LedgerState::apply_intent()` signature changes (e.g., new parameters added)
- New trait requirements on types (e.g., `impl Serialize for RuntimeTransaction`)
- Changed return types (e.g., `Result<T, E>` where error type `E` has changed)
- New fields in structs that need initialization (e.g., `TransactionContext` gains new fields)
- Method renames or moves to different modules (e.g., functions moving from `ledger::api` to `ledger::core`)

### Step 3: Rebuild Metadata

```bash
earthly -P +rebuild-metadata
```

This regenerates the runtime metadata that clients use to interact with the node. This needs to be re-run when:
- Pallet storage items change
- Extrinsic (callable functions) signatures change
- Runtime APIs are added or modified
- Custom types used in the runtime interface change

### Step 4: Rebuild Genesis (if needed)

```bash
earthly -P +rebuild-genesis
```

Required when:
- Runtime storage format changes
- New pallets are added
- Genesis configuration changes

The output of this will be a chain spec file which contains the hex of the genesis.

### Step 5: Run Tests

```bash
cargo test                    # Unit and integration tests
earthly +test                 # CI-equivalent tests (slow)
```

## Debugging Ledger Issues

### Keep midnight-ledger Checked Out

Maintain a local checkout of the midnight-ledger repository:

```bash
git clone https://github.com/midnightntwrk/midnight-ledger
```

**Why?** When you encounter ledger-related errors, you can search the source code directly:

```bash
cd midnight-ledger
# Search for error messages or types
rg "error message text"
rg "LedgerState"
```

The LedgerState implementation contains most of the critical logic. Understanding this code is essential for working on ledger-adjacent parts of the node.

### Common Debugging Techniques

**Error in transaction processing:**
1. Check the error message
2. Search midnight-ledger for the error text
3. Review LedgerState implementation
4. Check recent changes in ledger version

**State inconsistency:**
1. Verify genesis configuration
2. Check if metadata needs rebuilding
3. Review recent runtime changes

**Build failures after ledger upgrade:**
1. Check Cargo.toml for correct version pinning
2. Look for API changes in midnight-ledger changelog
3. Search for the failing function/type in midnight-ledger source

## Chain Specifications

### Working with Different Networks

```bash
# Build chain spec for local development
./target/release/midnight-node build-spec --disable-default-bootnode > chain-spec.json

# Convert to raw format
./target/release/midnight-node build-spec --chain chain-spec.json --raw > chain-spec-raw.json

# Start node with custom chain spec
./target/release/midnight-node --chain chain-spec-raw.json
```

### Available Networks

- **undeployed/local** - Local development, no AWS secrets required
- **qanet** - QA testing network, requires AWS secrets
- **preview** - Preview/staging network, requires AWS secrets
- **testnet** - Public testnet, requires AWS secrets

### AWS Secrets Limitation

If you don't have AWS access:
- You can only work with the `undeployed` network
- Cannot rebuild genesis for deployed networks (qanet, preview, testnet)
- For genesis rebuilds requiring secrets, contact the node team

When you need genesis rebuilt with secrets:
1. Open a PR with your changes
2. Ask the node team in Slack: "Could someone with AWS access run `earthly -P +rebuild-genesis` after downloading the secrets?"
3. A team member with AWS access will handle it

## Performance Testing

### Transaction Generator

See [toolkit README](../util/toolkit/README.md) for using the transaction generator to create test load.

### Benchmarking

```bash
# Runtime benchmarks (if enabled with runtime-benchmarks feature)
cargo build --release --features runtime-benchmarks
./target/release/midnight-node benchmark pallet --pallet pallet_name
```

## Hardfork Testing

**Note:** The hardfork testing process is currently incomplete. It was partially rewritten before the ledger v6 upgrade and never completed. Use the general upgrade testing approach documented in [testing-upgrades.md](testing-upgrades.md) instead.

## Quick Reference

| Task | Command |
|------|---------|
| Daily development | `cargo check`, `cargo test`, `cargo clippy` |
| Update metadata | `earthly -P +rebuild-metadata` |
| Rebuild genesis | `earthly -P +rebuild-genesis` |
| Build Docker image | `earthly +node-image` |
| List Earthly targets | `earthly doc` |
| Start dev environment | `nix develop` or `source .envrc` |
| Run local node | `CFG_PRESET=dev ./target/release/midnight-node` |

## Best Practices

1. **Use incremental builds:** Always prefer cargo over Earthly during development
2. **Keep ledger source handy:** Clone midnight-ledger locally for debugging
3. **Test before committing:** Run `cargo check && cargo test && cargo clippy && cargo fmt`
4. **Use Nix or direnv:** Don't manually manage environment variables
5. **Let CI handle complex builds:** Don't run full Earthly builds locally unless necessary
6. **Ask for help with secrets:** Don't try to work around AWS secret requirements
