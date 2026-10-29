#bridge #experimental
# Add opt-in Ethereum token bridge PoC runtime integration

Add caller-restricted Ethereum issuance and committed Compact escrow readers,
plus bounded retirement exports, authenticated result receipts and failure-only
retries. Wire the SDK outbound queue through an atomic attempt-checking processor
and commit its roots in Ethereum-compatible BEEFY/MMR leaves on fresh PoC chains.

Provide patched-dependency builds, separate/unified ledger tests, and a local
Geth/Lodestar and Midnight acceptance harness covering real-proof Compact use,
live consensus proofs, failed release, retry and settlement. Experimental relay
calls do not inherit governance's safe-mode privileges. Production weights,
existing-chain migration and durable relayer recovery remain outside this PoC.

PR: pending
