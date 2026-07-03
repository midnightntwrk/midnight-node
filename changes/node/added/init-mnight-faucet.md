#node #local-env
# Fund the local-env dev wallet (0x..01) via the c2m bridge on bring-up

Funds the well-known dev wallet (seed `0x..01`) with 1,000,000,000 NIGHT by driving
the real cNIGHT→NIGHT bridge on local-env bring-up. This is the "Part B" companion
to the #1778 genesis cNIGHT seeding: the `local` network ships an unfunded genesis,
so without this every wallet starts empty.

No dedicated image and no governance round:

- `mint-cnight-supply` (pre-genesis) now also sends the c2m bridge transfer with
  cardano-cli (mirroring `scripts/cnight-generates-dust/lock_to_ics.sh`). It **spends
  the seeded faucet cNIGHT** (the circulating pool it just minted) rather than minting
  fresh, so #1778's pools and the `M.U ≤ C.L` invariant are preserved. The transfer
  spends the seeding tx's outputs, so it lands strictly after the tx that anchors the
  bridge `initial_data_checkpoint` (and IS observed); `midnight-setup` then
  **pre-approves the tx hash in the c2m-bridge genesis config** — the new
  `approved_txs` genesis entry (#1809), threaded from `c2m-bridge-config.json` through
  the chainspec. No council/technical-committee approval is needed on the running
  chain.
- `init-mnight-faucet` (post-genesis): a plain shell script on the standard toolkit
  image that claims the transfer (`claim-rewards --claim-kind cardano-bridge` —
  feeless and self-signed by the seed, so the empty wallet needs no pre-existing
  balance or DUST), then **registers the wallet's DUST address**
  (`register-dust-address`, self-funded from the claimed NIGHT's retroactive DUST as
  it ages — no funding seed) so the wallet actually generates DUST and can transact.
  Idempotent via a `runtime-values/mnight-faucet-ready` marker.

The toolkit image is resolved like the node image (`local-environment/.envrc` derives
the content-hash tag; `+start-local-env-latest` builds it from source; CI passes it
explicitly), so the bring-up adds no extra image builds to CI.

PR: https://github.com/midnightntwrk/midnight-node/pull/1796
Issue: https://github.com/midnightntwrk/midnight-node/issues/1778
