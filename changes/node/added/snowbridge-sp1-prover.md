#bridge #experimental
# Add SP1 finality proving to the local Ethereum bridge PoC

Add a pinned SP1 BEEFY guest, local Groth16 prover, immutable Ethereum receiver,
and durable proof-job relaying for the opt-in fresh-genesis bridge harness.
Authenticate each signed next committee against the runtime MMR tip and enforce
the pinned successor at rotation. Reject stale, missing or inconsistent committee
witnesses; expose accepted committee commitments on Ethereum.
Preserve Compact token operations and the existing MMR/message and receipt checks.

PR: pending
