# Block Scanner

This is a Bun-based implementation of the block scanner for the midnight network block chain. It provides the ability to scan a midnight blockchain using the midnight Indexer. It collects all the blocks with some information (discarding the empty blocks) and stores the transactions in a data file, useful for later processing.

## Use case

Right now there are 2 use cases for this tool:
1. It gives the ability to scan the chain for data
2. It is used to prepare test data for the QA tests that target the midnight Indexer

## Prerequisites

- [Bun](https://bun.sh) installed on your system
- Access to a midnight network indexer endpoint (or undeployed environment setup locally)

## Installation

```bash
# Install dependencies
bun install
```

## Usage

### Basic Usage

```bash
# Run the scanner (defaults to undeployed environment)
bun run src/scanner.ts

# Run with specific environment
TARGET_ENV=devnet bun run src/scanner.ts

# Run to generate test data for the indexer tests
bun run src/scanner.ts <path/to/test/data/folder>

# Start scanning from a specific block height (must be less than latest)
START_BLOCK_HEIGHT=1000 TARGET_ENV=devnet bun run src/scanner.ts
```

### Available Environments (these might change in future)

- `undeployed` - Local development (ws://localhost:8088/api/v4/graphql/ws)
- `devnet` - Development network
- `qanet` - QA network

### Scripts

The following scripts are useful for development purpose
```bash
# Clean build artifacts
bun run clean

# Format code
bun run format

# Audit dependencies
bun run audit
```

These other scripts are needed to build and run the scanner
```bash
# Build the scanner
bun run build:scanner

# Run the scanner (default will target undeployed)
bun run scan

# Run the scanner against the selected target environment
TARGET_ENV=undeployed bun run scan
```

## Configuration

The scanner uses this information as configuration input:

- Environment variables for target environment (`TARGET_ENV`, defaults to `undeployed`)
- **START_BLOCK_HEIGHT** (optional) — Start scanning from this block height. Must be less than the latest block height. When set, overrides resume behavior and the blocks file is created/overwritten (not appended). Stats at end are still merged with any existing stats for that environment.
- WebSocket URLs for different networks (automatically extracted from the target environment)
- GraphQL subscription queries (available in a separate GraphQL file)

### Resume behavior

- **First run (no stats file for this environment):** The scanner performs a full sync from block 0 up to the latest block height. Block data is written to `tmp_scan/{TARGET_ENV}_blocks.jsonl`. At the end, stats are written to `stats/{TARGET_ENV}_stats.json`.
- **Subsequent runs (stats file exists):** The scanner resumes from the last scanned block height stored in `stats/{TARGET_ENV}_stats.json` (i.e. from `lastScannedBlockHeight + 1`). New blocks are appended to `tmp_scan/{TARGET_ENV}_blocks.jsonl`. At the end, the new run’s counts are added to the existing stats and the file is updated. If the chain has no new blocks, the scanner exits without subscribing.

Start height is chosen in this order: `START_BLOCK_HEIGHT` (if set) → resume from stats file → 0.

## Output

The scanner creates:
- `tmp_scan/` directory for block data files
- `tmp_scan/{TARGET_ENV}_blocks.jsonl` — Raw block data (overwritten on first run or when using `START_BLOCK_HEIGHT`; appended when resuming)
- `stats/` directory for per-environment scan statistics
- `stats/{TARGET_ENV}_stats.json` — Persisted stats used for resume and cumulative totals. Schema:
  - `lastScannedBlockHeight` — Last block height received (used to resume from `lastScannedBlockHeight + 1`)
  - `totalBlocksScanned` — Total blocks scanned (cumulative across runs when resuming)
  - `blocksWithTransactions` — Number of blocks that had transactions
  - `totalTransactionsFound` — Total transaction count
  - `contractActionsFound` — Number of blocks with contract actions
  - `totalScanDurationSeconds` — Cumulative scan duration in seconds
  - `lastUpdated` — ISO timestamp of last update
- `*.jsonc` — A number of jsonc files created from the templates which are needed as Indexer test data (when a test data folder path is provided):
  - `blocks.jsonc` — genesis/latest and up to 100 recent block hashes
  - `transactions.jsonc` — regular and system transaction hashes/identifiers
  - `contract-actions.jsonc` — contracts having both a deploy and a call, with per-action block references
  - `contract-events.jsonc` — contracts that emitted public contract events (MIP-0002), with the distinct event types each emitted. The contract addresses discovered in the scan are enriched via the indexer's `contractEvents` query; a schema probe decides support, so on environments whose indexer predates that query the file is left untouched (a curated fixture is not destroyed), while transient query failures are retried and then fail the run rather than being mistaken for missing support. Because contract addresses do not survive a chain reset, re-running `bun run generate:data` after a reset refreshes this file (and the others) to the live chain.
  - `unshielded-token-types.jsonc` — the unshielded token types the scanned range holds, so the custom unshielded token e2e suite can pick a token the funding wallet is able to spend. Every unshielded UTXO of the scan is grouped by token type and owner and kept when still unspent; NIGHT (the all-zero token type) is summarised only, since it is on every chain and dominates every scan. The entries are **candidates, not guarantees**: "unspent" is point-in-time, so a recorded UTXO can be spent right after the scan and holdings created outside the scanned range are not seen — consumers probe the live balance and skip when it is gone, and several candidates are recorded so a single spend does not disable a suite.

## Troubleshooting

### Common Issues

1. **WebSocket Connection Issues**: Ensure the indexer endpoint is accessible
2. **Permission Errors**: Make sure you have write permissions for the output directory
3. **Memory Issues**: For large datasets, consider processing in smaller batches

### Debug Mode

Enable debug logging by setting the log level:

```bash
DEBUG=* bun run scan
```

## Performance Notes

- Bun typically provides 2-3x performance improvement over Node.js
- Memory usage is generally lower
- Startup time is significantly faster
- Built-in bundling eliminates the need for separate build steps

## Contributing

When making changes:
1. Ensure code follows the existing patterns
2. Test with multiple environments
3. Update documentation as needed
4. Consider performance implications
