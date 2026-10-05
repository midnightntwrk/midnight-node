#node #toolkit
# Decode wide-integer datums and skip undecodable ones in db-sync data sources

Enables cardano-serialization-lib's `arbitrary-precision-json` feature so datums
containing integers outside `u64`/`i64` decode instead of being rejected.

The candidate and cNIGHT mapping-validator data sources now decode each row's
datum after the query rather than inside it, so an output whose datum cannot be
decoded is logged and skipped like any other invalid entry, instead of failing
the whole query. The same change is applied to the partner-chains db-sync
candidate data source.

PR:
Issue:
