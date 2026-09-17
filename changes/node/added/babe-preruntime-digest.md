#node

# Emit BABE SecondaryPlain pre-runtime digest on every AURA block

Block authors now attach a BABE `SecondaryPlain` pre-runtime digest to every
block they produce with AURA. The digest carries the block's AURA slot and the
same author index as AURA PreDigest.

- Adds `BabePreDigestProposerFactory`, a proposer wrapper that reads the AURA
  authority count at the parent and appends the pre-digest.
- It is attached unconditionally, with no runtime gate. Runtimes that predate
  `pallet-consensus-engine` have no `pallet-babe` either, so nothing reads the
  item and it is inert there; from the upgrade that adds the pallet on, the
  runtime *requires* it on every block. Node binaries must therefore be rolled
  out before that runtime upgrade — a node without this wrapper cannot author
  once the upgrade lands.

PR: https://github.com/midnightntwrk/midnight-node/pull/1951
Issue: https://github.com/midnightntwrk/midnight-node/issues/1751
