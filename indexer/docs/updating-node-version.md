# Updating Node Version Guide

This guide ensures complete and correct updates when changing the Midnight Node version that the indexer supports.

## Overview

When updating to a new node version, multiple files must be updated in sync. Missing any of these will cause the indexer to fail in production environments.

## Required Changes Checklist

When updating from an old version (e.g., `0.13.2-rc.2`) to a new version (e.g., `0.13.5-79c649d7`):

### 1. Generate and Add Node Metadata
```bash
# First, update the NODE_VERSION file to the new version
echo "0.13.5-79c649d7" > NODE_VERSION

# Then generate node data for the new version
just update-node
```

#### Test Files (if present)
- `chain-indexer/src/infra/subxt_node.rs` - Update test data if needed (line ~638 in test_finalized_blocks_0_13)

### 3. Clean Up Old Metadata (Optional)
```bash
# Remove old metadata directory after confirming new version works
rm -rf .node/<old-version>/
```

### 4. Test Locally

```bash
# Run tests to ensure metadata loads correctly
just test

# Run the indexer locally against a node
just run-node
# In another terminal
just run

# Optional: Run the specific e2e test
cargo test -p indexer-tests native_e2e
```

### 5. Verify Changes

Before creating PR, verify:
- [ ] `NODE_VERSION` file updated
- [ ] Metadata file exists at `.node/<new-version>/metadata.scale`
- [ ] All tests pass
- [ ] No references to old version remain (check with ripgrep)

## Common Mistakes to Avoid

1. **Adding metadata without updating code** - The metadata file alone is not enough
2. **Forgetting test files** - Tests will fail in CI if not updated
3. **Manual searching** - Always use ripgrep; manual searches miss occurrences
4. **Not testing locally** - Local testing catches most issues before PR

## Breaking Changes

If the new node version includes breaking changes, follow these steps:

### Common Breaking Change Scenarios

#### Field Removal
Example: Node removes `new_registrations` field
1. **Compilation error**: `error[E0560]: struct has no field named 'new_registrations'`
2. **Fix**: Update domain types in `indexer-common/src/domain/`
3. **GraphQL**: Update schema if the field was exposed
4. **Database**: Consider migration if the field was stored

#### Transaction Format Change
Example: Node changes from hex-encoded to binary transactions
1. **Runtime error**: `cannot hex-decode transaction: odd number of digits`
2. **Fix**: Update parsing in `chain-indexer/src/infra/subxt_node/`
3. **Test**: Verify with real transactions from the new node

#### Event Structure Change
Example: Event adds/removes fields
1. **Compilation error**: Missing or extra fields in event destructuring
2. **Fix**: Update event handling in `chain-indexer/src/domain/`
3. **Storage**: Update database schema if events are stored


## CI Considerations

The CI will fail if:
- Metadata file is missing
- Version mismatches exist between files
- Tests reference non-existent node directories

## Rollback Procedure

If issues are discovered after deployment:
1. Revert the PR
2. Keep the new metadata file (doesn't hurt)
3. Ensure all references point back to working version
4. Investigate and fix before re-attempting
