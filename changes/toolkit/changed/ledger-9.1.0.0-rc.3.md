#toolkit
# Adapt toolkit to ledger 9.1.0.0-rc.3

rc.3 makes `ContractOperationVersionedVerifierKey` dual-versioned — `V3` now
takes a 2.x (`transient_crypto_old`) verifier key for v1 circuits, and `V4` the
3.x key for v2 — so the toolkit's contract-maintenance verifier-key insert is
updated to construct the version that matches the circuit's proof stack.

PR: https://github.com/midnightntwrk/midnight-node/pull/1738
Issue: https://github.com/midnightntwrk/midnight-node/issues/1737
