#node
# Add cross-chain bridge pool invariant e2e suite (state monitor + cooperative flood)

Adds an independent cross-chain invariant suite for the cNIGHT<->NIGHT bridge
(`tests/e2e/tests/c2m_bridge_invariants.rs`), serialized behind `C2M_BRIDGE_SERIAL`
and local-env only. Invariants are state predicates over aggregate pool balances, so
the monitor needs no per-transaction knowledge:

- Midnight pools (M.R / M.L / M.U) via the toolkit's `night_pools()` genesis replay
  (a new reusable `show_night_pools::read_night_pools`).
- Cardano pools: C.L (ICS validator) and C.R (Reserve validator) via ogmios, and
  `C.U = minted_total - C.L - C.R` with `minted_total` read from kupo (new
  `KupoClient`). Adds `reserve_validator_address()` mirroring `ics_validator_address()`.

`check_cross_chain_invariants` asserts the four relations — `M.U <= C.L`,
`C.U <= M.L`, `C.R <= M.R`, `M.U + C.U <= S` — as continuous inequalities, with strict
equalities at quiescence (accounting for the subminimal wait-pool). A cooperative,
seeded flood spends pre-seeded circulating cNIGHT (no minting) into a diverse mix of
approved / unapproved / invalid / subminimal transfers via a new
`CardanoClient::make_cooperative_bridge_transfer`, asserting at genesis -> through the
flood -> at quiescence. A manual `#[ignore]` negative control mints unmatched cNIGHT to
prove the monitor catches a breach.

Requires the Part A cNIGHT genesis seeding (#1778) and a clean local-env (no prior
cNIGHT minting). `reqwest` is now an unconditional dev-dependency of the e2e crate
(also used by the kupo client).

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/1779
