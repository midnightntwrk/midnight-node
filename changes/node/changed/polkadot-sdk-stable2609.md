#node #runtime #polkadot-sdk

# Update Polkadot SDK to stable2609

Moves the whole polkadot-sdk dependency set from tag `polkadot-stable2606` to branch
`stable2609`.

The bump is what makes the AURA→BABE migration implementable node-side: `stable2609` carries the
backport of paritytech/polkadot-sdk#13061, which exposes `sc_consensus_babe::build_verifier` /
`BuildVerifierParams` (the AURA equivalent). A BABE verifier can therefore be composed into the
node's own import queue instead of only through `sc_consensus_babe::import_queue`, which is what
lets both engines share one queue — see
`changes/node/changed/aura-babe-engine-dispatch.md`.

Call-site changes that come with the bump:

- `sc_service::build_network` gained a `gap_sync_body_policy` parameter and returns an extra
  bitswap handle (midnight node and the partner-chains demo node).
- Native runtime execution is gone upstream, so `native_version()` / `sp_version::NativeVersion`
  are removed from the runtime.
- CI images build with `OPENSSL_NO_VENDOR=1`: `sc-network`'s `litep2p` pulls in `str0m` with its
  `vendored` OpenSSL feature, which compiles OpenSSL from source via perl, and AL2023-minimal's
  perl is incomplete (`Configure` exits 2). `openssl-devel` is already in the CI image, so the
  system library is used instead.

PR: https://github.com/midnightntwrk/midnight-node/pull/2113
Issue: https://github.com/midnightntwrk/midnight-node/issues/1757
