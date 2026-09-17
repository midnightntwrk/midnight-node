#node
# Revert the config-gated tblock correction; it ships runtime-gated in 1.0.300

Reverts #1932, #1965 and #1995. The correction for the first transaction of a block being
verified at the wrong `tblock` (#1924) was first cut here as a node-config change with a dated
cutoff (`tblock_correction_offset`, `tblock_correction_disable_after`), which put two
consensus-critical values in `default.toml`. It was reworked to be gated on the on-chain runtime
and shipped on the 1.0.300 line instead; the node-1.0.2 build deployed to mainnet never carried 
the config-gated version. This puts the `release/node-1.0.2` tree back in step with that build: 
ledger 8.1.2 and nothing else. Both config keys are removed.

PR: https://github.com/midnightntwrk/midnight-node/pull/TBD
Issue: https://github.com/midnightntwrk/midnight-node/issues/1924
