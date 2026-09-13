#toolkit #fix
# Sender no longer reports landed transactions as failed after fork retractions

When a transaction's block was retracted by a fork and the watch stream never
surfaced the re-inclusion, `toolkit send` waited out the finalization timeout
and exited `FAILED_TO_FINALIZE` even though the transaction had finalized in
another block. On watch timeout the sender now scans the finalized chain
directly for the extrinsic (up to 64 blocks below the finalized head) and
reports `FINALIZED_AFTER_RETRACTION` with a success exit when found.
Retractions are also logged as they happen, and the watch phases accept
environment overrides (`MN_SEND_BEST_BLOCK_TIMEOUT`,
`MN_SEND_FINALIZED_TIMEOUT`, seconds) for slow or fault-injected
environments.

PR: https://github.com/midnightntwrk/midnight-node/pull/1927
Issue: https://github.com/midnightntwrk/midnight-node/issues/1854
