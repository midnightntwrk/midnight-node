#node
# Restore the preview chain-spec that live preview actually runs

Preview was reset in June 2026 (#1690) to fix an empty Locked pool, and the regenerated
chain-spec and genesis state landed in #1699 — but only on `release/node-1.0.1`. The 2.x line
branched before that, so `main` and `release/node-2.1.0` still shipped the pre-reset artifacts.

A preview node brought up from an empty disk using `res/preview/` computed genesis
`0x801d…b880` instead of live preview's `0x3c096de2…6dd13796`, was rejected by every bootnode as
a different chain, and sat at block #0 with no peers. Nodes already running were unaffected —
their genesis is on disk.

Forward-ports the five artifacts from `release/node-1.0.1`, leaving `res/preview/` byte-identical
to the branch preview runs. The input configs (`ics-config.json`, `reserve-config.json`) already
matched and are untouched.

Closes: #1690
