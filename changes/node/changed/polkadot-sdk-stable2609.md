#node #runtime

# Update Polkadot SDK to polkadot-stable2609

Updates the Polkadot SDK dependency set from the `polkadot-stable2606` tag to
`polkadot-stable2609`. Until the final release is published the pin points at the
latest release candidate tag (`polkadot-stable2609-rc1`); it will be moved to the
final `polkadot-stable2609` tag without further code changes.

stable2609 includes two fixes previously carried on the temporary shieldedtech fork:

- paritytech/polkadot-sdk#12506: `GrandpaBlockImport::import_justification` verifies and
  enacts the justification under one authority-set lock, removing a double-finalization
  panic at authority-set change blocks.
- paritytech/polkadot-sdk#12754: `sc_consensus_babe::prune_finalized` skips epoch pruning
  when the finalized header has no BABE pre-digest, so `BabeBlockImport` can be
  constructed on an AURA chain.

Code adjustments for the new SDK API:

- `sc_service::build_network` now returns a fifth element (the bitswap handle) and takes a
  `gap_sync_body_policy` field in `BuildNetworkParams`; both node services (midnight and the
  partner-chains demo) are updated.
- `sp_version::NativeVersion` was removed upstream; the unused `native_version()` helper is
  dropped from the runtime.

CI: litep2p (via `sc-network`) now enables str0m's vendored OpenSSL feature, which builds
OpenSSL from source with perl. The CI image's perl lacks `FindBin.pm`, so the Earthfile sets
`OPENSSL_NO_VENDOR=1` to keep linking against the system OpenSSL already installed there.

PR:
Issue:
