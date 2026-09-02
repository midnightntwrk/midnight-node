# Ledger 8 → 9 Devnet Rehearsal

This runbook is the release gate for an indexer build that supports the ledger
8 → 9 hard fork. It uses a fresh ledger-8 dev chain, submits ledger-8 traffic,
enacts the ledger-9 runtime through governance, then submits ledger-9 traffic.
The indexer must continue indexing across the boundary.

## Required inputs

- A candidate indexer image tag, built from the release commit.
- A ledger-8 node image and matching toolkit image (normally `1.0.0`).
- The intended ledger-9 migration-node image and matching toolkit image.
- The authoritative compressed runtime WASM produced by that migration-node
  build. Supplying it explicitly prevents a binary-only image swap from causing
  a no-op upgrade with an embedded ledger-8 WASM.

## Run

From the repository root:

```bash
IMAGE_REGISTRY=ghcr.io/midnight-ntwrk \
INDEXER_TAG=4.4.0-rc.2 \
FROM_NODE_TAG=1.0.0 \
TO_NODE_TAG=<ledger-9-node-tag> \
FROM_TOOLKIT_TAG=1.0.0 \
TO_TOOLKIT_TAG=<ledger-9-toolkit-tag> \
RUNTIME_WASM=/path/to/midnight_node_runtime.compact.compressed.wasm \
bash qa/scripts/test-hardfork-8to9.sh
```

The script is non-interactive by default. Set `AUTO=0` to pause before the
pre-fork traffic, runtime upgrade, and post-fork traffic.

## Acceptance criteria

- The chain begins below runtime `specVersion` `2_000_000`.
- Ledger-8 transactions are accepted and indexed before the upgrade.
- Governance changes the runtime to `specVersion` `2_001_000` or later.
- Ledger-9 transactions are accepted and indexed after the upgrade.
- `chain-indexer` logs contain none of: `translate ledger state`, `ledger state
  root mismatch`, or `zswap state root mismatch`.
- The script completes with its success message and records a post-fork indexed
  height.

## Release checklist

- Record the exact node, toolkit, indexer image, and runtime-WASM provenance in
  the QA evidence.
- Run smoke and integration QA against the candidate image and intended node /
  toolkit pair.
- Obtain QA sign-off for this rehearsal before tagging the release candidate.
