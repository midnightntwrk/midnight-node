#toolkit #ledger #audit
# Harden WalletSeed hex decoding and reject mismatched maintenance address slices

Follow-up to the Least Authority A2 Suggestion 3 work in #1217, which hardened
`WalletSeed`'s traits but left the decode paths untouched.

- `try_from_hex_str` now validates the length (32, 64 or 128 hex characters)
  *before* decoding, so untrusted input is rejected on size alone. It previously
  decoded first and checked the byte count afterwards.
- Both `try_from_hex_str` and `try_from_lazy_hex` accept an optional `0x`
  prefix, matching how seeds are written everywhere else in the toolkit.
- Neither path allocates any more: hex is decoded straight into the fixed-size
  array behind the variant via `hex::decode_to_slice`, so seed bytes never land
  in an intermediate `Vec` that outlives `Zeroize`.
- `MaintenanceUpdateBuilder::add_addresses` returns
  `Result<(), MaintenanceUpdateError>` and rejects slices of differing length.
  The `zip` introduced in #1217 removed the panic but silently dropped the tail
  of the longer slice; a caller that miscounted got a partial update and no
  signal. There are no production callers yet, so nothing downstream changes.

PR: https://github.com/midnightntwrk/midnight-node/pull/2142
Ticket: https://shielded.atlassian.net/browse/PM-22038
