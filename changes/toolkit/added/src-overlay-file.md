#toolkit
# Add `--src-overlay-file` to generate-txs sources

Overlay transactions/blocks from file(s) on top of the main source. Unlike
`--src-file` (which replaces the source and conflicts with `--src-url`), this
composes with `--src-url`: the generator's initial state becomes the live chain
plus the overlaid, typically not-yet-submitted, transactions.

Enables building a tx against another tx that never goes on-chain — e.g. a
contract `store` call against a deploy that was generated but deliberately not
submitted, so submitting the store hits ContractNotPresent (the DDoS-rejection
e2e tests).

PR: https://github.com/midnightntwrk/midnight-node/pull/1796
