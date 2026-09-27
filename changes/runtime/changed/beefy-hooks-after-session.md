#runtime #beefy #mmr

# Run the BEEFY and MMR hooks after Session

`#[frame_support::runtime]` runs hooks in pallet-index order, so Mmr (22)
appended its leaf before Session (30) rotated the BEEFY sets. The leaf of a
session's first block, the BEEFY mandatory block, then named the current set
instead of the next one (polkadot-fellows/runtimes#160), and a light client
could not hand over through a session whose only BEEFY justification is its
mandatory block.

Beefy, Mmr and BeefyMmrLeaf move to pallet indices 34, 35 and 36; Session keeps
its place relative to every other pallet. Storage prefixes are by name, so no
storage changes; the three pallets' call and event indices change.
