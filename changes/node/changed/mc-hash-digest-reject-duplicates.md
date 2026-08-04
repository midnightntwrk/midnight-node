#node
# Reject block headers carrying duplicate main chain reference hash digests

`McHashInherentDigest::value_from_digest` now returns an error when a header
contains more than one `mcsh` pre-runtime digest item, instead of silently
using the first match. polkadot-sdk does not deduplicate or bound pre-runtime
digest items, so this guards against ambiguity if any consumer reads the digest
with different selection logic, mirroring how sc-consensus-aura's
`find_pre_digest` rejects a second slot pre-digest. Honest blocks carry exactly
one entry and are unaffected; this is a strict tightening of block validation.

PR: https://github.com/midnightntwrk/midnight-node/pull/1617
Issue: https://github.com/midnightntwrk/midnight-node/issues/1616
