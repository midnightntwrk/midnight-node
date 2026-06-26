#node
# Seed cNIGHT on the Cardano side of local-env at genesis (bridge invariants)

On local-env the Midnight side carries the full NIGHT pools at genesis (reserve /
locked / unlocked) but the Cardano side started with **zero cNIGHT**, so the
cross-chain bridge pool invariants (e.g. `M.U <= C.L`) were violated from genesis
and could not be asserted end-to-end.

Adds a `cnight-seeder` step to local-env that mints the full cNIGHT supply (24B)
and distributes it so the Cardano pools mirror the Midnight pools at genesis:
`C.R = M.R` to the Reserve validator, `C.L = M.U` to the ICS validator, and
`C.U = M.L` to the funded (circulating) address. It runs after the reserve/ICS
validators are deployed and before `midnight-setup` captures the bridge
observation checkpoint, so the seeded ICS cNIGHT is treated as pre-existing
locked supply rather than swept to Treasury.

Also fixes `midnight-setup` to patch the bridge's ICS / Reserve validator
addresses from the freshly-deployed `contracts-info.json` (mirroring the existing
Council / Tech-Auth patching) instead of using the stale static
`res/local-environment/{ics,reserve}-config.json` addresses, so the bridge
observes the same validators the seeder funds.

This unblocks the cross-chain pool invariant e2e suite (#1773).

PR: https://github.com/midnightntwrk/midnight-node/pull/1780
Issue: https://github.com/midnightntwrk/midnight-node/issues/1778
