#runtime
# Validate DustPublicKey at the cNight-observation IDP boundary

Registrations carrying a DustPublicKey whose length is within the wire envelope
but whose value is out of range for the Bls12-381 Fr scalar field used by the
dust circuit are filtered at the inherent-data-provider boundary, gated on
`CNightObservationApi` v3. At v2 and earlier the legacy ingestion path is
preserved for consensus equivalence across the runtime-upgrade window; the
`log::error!("Fatal: ...")` lines in `pallet-cnight-observation` that fired
when the downstream `AssetCreate` / `AssetSpend` could not construct a
`CNightGeneratesDustEvent` are downgraded for the `DustPublicKey` deserialise
variant — `debug` severity, no `Fatal:` prefix — so legacy-runtime + new-binary
pairings also stop emitting misleading "Fatal" log lines. Other ledger error
variants at the same call sites continue to be reported, but at `warn` severity
(not `error`) and without the misleading prefix. Runtime metadata rebuild is
required for the v2 → v3 API version bump.

PR: https://github.com/midnightntwrk/midnight-node/pull/1489
Ticket: https://shielded.atlassian.net/browse/PM-22301
Helps with: https://github.com/shieldedtech/shielded-security-engineering/issues/233
