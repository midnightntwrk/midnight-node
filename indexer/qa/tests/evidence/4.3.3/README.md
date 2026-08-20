# Indexer QA Test Execution and Report Summary — 4.3.3

## Overview

| Field | Value |
|---|---|
| Date | 5 June 2026 |
| QA Contact | Giuseppe Salvatore (giuseppe.salvatore@shielded.io) |
| Indexer version | 4.3.3 |
| Node version | 1.0.0 |
| Toolkit version | 1.0.0 |
| QA Sign-off | ✅ |

## Test Execution Summary

```
┌─────────────┬─────────────┬────────┬────────┬─────────┬───────┐
│ Environment │ Suite       │ Passed │ Failed │ Skipped │ Total │
├─────────────┼─────────────┼────────┼────────┼─────────┼───────┤
│ undeployed  │ e2e         │     38 │      0 │       0 │    38 │
│ qanet       │ smoke       │     18 │      0 │       1 │    19 │
│ qanet       │ integration │    168 │      0 │       2 │   170 │
├─────────────┼─────────────┼────────┼────────┼─────────┼───────┤
│ Total       │             │    224 │      0 │       3 │   227 │
└─────────────┴─────────────┴────────┴────────┴─────────┴───────┘
```

All suites passed; no failures. The skipped/todo entries are the suites' own
conditional cases, not errors.

## Test Report Details

For details and the source result files used to generate this retport please look at the env folders


## Test Execution Guide

All commands are executed from `qa/tests/`. For hosted environments (e.g.
`qanet`), `FUNDING_SEED_<ENV>` is required and must be a wallet that has
performed **both** shielded and unshielded transactions. For undeployed e2e the funding seed is not required, as the seeds are known.

```bash
# undeployed — e2e
export TARGET_ENV=undeployed
export INDEXER_TAG=4.3.3
export NODE_TAG=1.0.0
export NODE_TOOLKIT_TAG=1.0.0
export XRAY_COMPONENT=indexer
export VITEST_MAX_WORKERS=4
bun run test:e2e

# qanet — smoke
export TARGET_ENV=qanet
export FUNDING_SEED_QANET=<wallet seed with shielded + unshielded txs>
export XRAY_COMPONENT=indexer
export VITEST_MAX_WORKERS=4
bun run test:smoke

# qanet — integration
export TARGET_ENV=qanet
export FUNDING_SEED_QANET=<wallet seed with shielded + unshielded txs>
export XRAY_COMPONENT=indexer
export VITEST_MAX_WORKERS=4
bun run test:integration
```
