#toolkit #ledger9
# Fix v6/v7 verifier-key dispatch in ledger-9 contract maintenance

`contract-maintenance --upsert-entrypoint` hardcoded the v7-only verifier-key slot,
so upserting a key compiled by the currently-pinned compactc (which still emits v6)
failed with a header-tag mismatch.

- Upsert now peeks the key's tag and dispatches to the v6 or v7 slot, mirroring the
  logic contract deploy already used.
- Entry-point removal now inspects the existing on-chain operation to target
  whichever slot the key actually lives in, instead of assuming v7.

PR: https://github.com/midnightntwrk/midnight-node/pull/1711
