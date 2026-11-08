#toolkit

# Add `show-night-pools` command

Dump the NIGHT pools (Reserved / Locked / Unlocked) and the full supply breakdown
(block reward pool, unclaimed block rewards, UTXOs, contracts) held in a network's
`LedgerState`. These totals are not exposed by any RPC, so the command reconstructs
the full state the same way `dust-balance` does - by replaying blocks from the
`Source` (RPC + fetch cache) into a version-correct `LedgerContext` - then reads the
pools off the rebuilt `LedgerState`. The ledger version is detected from the replayed
blocks, so no version flag is needed. Reuses `--src-url` / `--fetch-cache` /
`--ledger-state-db` from the shared `Source` args.

PR: https://github.com/midnightntwrk/midnight-node/pull/1726
Issue: https://github.com/midnightntwrk/midnight-node/issues/1725
