#node #local-env
# Fund the local-env dev wallet (0x..01) via the c2m bridge on bring-up

Adds a `init-mnight-faucet` docker-compose job that funds the well-known dev wallet
(seed `0x..01`) with 1,000,000,000 NIGHT by driving the real cNIGHT→NIGHT bridge
end-to-end against the running stack. This is the "Part B" companion to the #1778
genesis cNIGHT seeding: the `local` network ships an unfunded genesis, so without
this every wallet starts empty.

- The funding **spends the seeded faucet cNIGHT** (the circulating pool minted by
  the `mint-cnight-supply` step) rather than minting fresh, so #1778's pools and the
  `M.U ≤ C.L` invariant are preserved — each transfer only moves cNIGHT faucet→ICS.
- The Cardano tx hash is pre-approved through the local-env governance flow
  (council + technical-committee `root_call`, reusing the c2m_bridge e2e helpers,
  no new toolkit command) before submission, so it is treated as an approved user
  transfer (not swept to Treasury). The final `ClaimRewards(CardanoBridge)` is
  feeless and self-signed by the seed, so the empty wallet claims its bridged
  NIGHT with no pre-existing balance or DUST.
- After the claim it **registers the wallet's DUST address** (toolkit
  `register-dust-address`, self-funded from the NIGHT's retroactive DUST — no
  funding seed) so the wallet actually generates DUST and can transact. The
  just-claimed NIGHT is aged a few blocks first so its retroactive-DUST budget
  covers the registration fee.
- Implemented as a feature-gated e2e test `tests/e2e/tests/init_mnight_faucet.rs`
  (`init-mnight-faucet` feature, built alongside `local`) so it never joins the normal
  `cargo test` sweep; `tests/e2e/src/config.rs` gains `E2E_NODE_URL`/`E2E_OGMIOS_URL`
  overrides so it reaches the stack by compose service name.
- The test is compiled + stripped into a slim image that CI builds and pushes,
  content-hash tagged, to `ghcr.io/midnight-ntwrk/local-env-init-mnight-faucet:<tree-hash>-<arch>`
  (via `+images`, mirroring `+node-image`/`+toolkit-image`). The local-env runner resolves the
  tag for the checkout and pulls it (building locally via `earthly +init-mnight-faucet-image`
  only as a fallback for an unpublished tree), and `+local-env-ci` pulls it into the gate. The
  job runs once after the chain is up (after `midnight-setup` + `mint-cnight-supply`) and is
  idempotent via a `runtime-values/mnight-faucet-ready` marker.

PR: https://github.com/midnightntwrk/midnight-node/pull/1796
Issue: https://github.com/midnightntwrk/midnight-node/issues/1778
