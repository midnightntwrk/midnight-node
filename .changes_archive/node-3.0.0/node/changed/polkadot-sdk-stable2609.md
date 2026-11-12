#node #runtime

# Update Polkadot SDK to polkadot-stable2609

Updates the Polkadot SDK dependency set from the `polkadot-stable2606` tag to
Shielded fork of polkadot-sdk at tag `midnight-polkadot-stable2609`. This tag is
paritytech polkadot-sdk tag `polkadot-stable2609` with added fix commit:
'rpc-server: set TCP_NODELAY on JSON-RPC connections'.

Code adjustments for the new SDK API:

- `sc_service::build_network` now returns a fifth element (the bitswap handle) and takes a
  `gap_sync_body_policy` field in `BuildNetworkParams`; both node services (midnight and the
  partner-chains demo) are updated.
- `sp_version::NativeVersion` was removed upstream; the unused `native_version()` helper is
  dropped from the runtime.

CI: litep2p (via `sc-network`) now enables str0m's vendored OpenSSL feature, which builds
OpenSSL from source with perl. The CI image's perl lacks `FindBin.pm`, so the Earthfile sets
`OPENSSL_NO_VENDOR=1` to keep linking against the system OpenSSL already installed there.

PR: https://github.com/midnightntwrk/midnight-node/pull/2141
PR: https://github.com/midnightntwrk/midnight-node/pull/2288
Issue: https://github.com/midnightntwrk/midnight-node/issues/1757
