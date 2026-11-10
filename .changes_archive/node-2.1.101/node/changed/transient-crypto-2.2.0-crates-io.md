#node #ledger #security
# Resolve the 2.x midnight-transient-crypto to crates.io 2.2.0

The 2.x `midnight-transient-crypto` (the one the ledger 8 stack and
`midnight-zkir`'s `transient-crypto-old` use) was pinned to a git tag that
predates the verifier-key fix shipped with ledger 8.1.2: `VerifierKey` now
keeps the bytes it was decoded from, so serializing it is a function of the
value and no longer changes once the key is initialized in place, and it
rejects keys with trailing bytes so two encodings cannot name one key. The
crates.io 2.2.0 release carries that fix, so the git override is dropped and
the crate resolves to the registry. Same version number, different source:
the lockfile moves from a git source to the crates.io checksum.
