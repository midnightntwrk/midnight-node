#node #toolkit
# Decode wide-integer datums and skip undecodable ones in db-sync data sources

Enables cardano-serialization-lib's `arbitrary-precision-json` feature so datums
containing integers outside `u64`/`i64` decode instead of being rejected.

The candidate and cNIGHT mapping-validator data sources now decode a datum that
is not valid Plutus data, or is nested too deeply to parse (64 or more nested
lists), as absent, so that output is skipped like any other invalid entry
instead of failing the whole query. The same change is applied to the
partner-chains db-sync candidate data source.

Candidate registration `tx_inputs` are now returned in a deterministic order.

PR: https://github.com/midnightntwrk/midnight-node/pull/2382
