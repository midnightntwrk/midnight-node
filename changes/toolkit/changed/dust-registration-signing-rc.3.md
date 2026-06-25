#toolkit
# Sign dust registrations over the assembled intent for ledger 9-rc.3

rc.3 folds `dust_actions` into `Intent::data_to_sign` (the new
`IntentSigningEnvelope`), so a dust registration's `night_key`,
`dust_address` and `allow_fee_payment` are now part of the signed payload.
The toolkit was computing `data_to_sign` *before* attaching the registrations
to the intent, so the signature no longer matched at validation and genesis
generation failed with `InvalidDustRegistrationSignature`.

Both sign-paths now attach the registrations (unsigned) to the intent first,
then compute `data_to_sign` and fill in each signature, mirroring the ledger's
own `Transaction::sign`:

- `util/toolkit/src/genesis_generator.rs` (`add_dust_actions`)
- `ledger/helpers/src/versions/common/transaction.rs` (`apply_dust`, plus a new
  `DustRegistrationBuilder::build_unsigned`)

PR: https://github.com/midnightntwrk/midnight-node/pull/1738
Issue: https://github.com/midnightntwrk/midnight-node/issues/1737
