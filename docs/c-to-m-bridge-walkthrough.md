# C-to-M bridge — Happy Path walkthrough (Stagenet)

A hands-on guide to exercising the **Cardano-to-Midnight (C2M) bridge** end to end
on **Stagenet**: lock cNIGHT on Cardano, watch Midnight observe and credit it, and
claim the bridged mNIGHT.

> **Who this is for.** People who want to drive the bridge themselves and see it
> work — not just read about it. It walks the *User Transfer* happy path only.
> For the concepts (Reserve vs User transfers, subminimal/invalid/unapproved
> variants, and how the two pallets fit together) read
> [`docs/c-to-m-bridge.md`](./c-to-m-bridge.md) first — this guide assumes it.
>
> **The executable source of truth** for this flow is the e2e test
> [`tests/e2e/tests/c2m_bridge.rs`](../tests/e2e/tests/c2m_bridge.rs)
> (`bridge_transfer_cnight_to_midnight_address`). Every command below mirrors a
> step that test performs against a live chain. If a command here ever drifts, the
> test is what's real.

> **This guide is just one example — the toolkit is a convenience, not the only
> way.** Everything the `bridge-transfer` command does on the Cardano side is
> ordinary Cardano tx construction: send cNIGHT to the ICS address with the
> recipient in metadata label `6500973`. You can achieve the identical result by:
>
> - **Crafting the tx with `cardano-cli`** and submitting it through your own
>   Cardano node socket — see the worked example
>   [`scripts/cnight-generates-dust/lock_to_ics.sh`](../scripts/cnight-generates-dust/lock_to_ics.sh).
> - **Using a wallet/dApp UI** — e.g. the demo dApp built for this purpose:
>   [midnightntwrk/bridge-demo-dapp#3](https://github.com/midnightntwrk/bridge-demo-dapp/pull/3)
>   ([demo video](https://drive.google.com/file/d/1uw0jD8INJexyoievtYOYhKV1zY-pfOld/view)).
>
> The Midnight side (observation, approval, claim) is the same regardless of how
> the Cardano lock was produced.

---

## The happy path at a glance

A *User Transfer* is a Cardano wallet locking cNIGHT at the bridge and a Midnight
address later claiming the equivalent mNIGHT:

```
                CARDANO (preview)                          MIDNIGHT (Stagenet)
 ┌────────────────────────────────────────┐     ┌────────────────────────────────────────────┐
 │ 1. Lock cNIGHT at the ICS validator    │     │                                            │
 │    address, with the recipient's       │     │                                            │
 │    32-byte Midnight address in tx      │     │                                            │
 │    metadata (label 6500973).           │     │                                            │
 │              │  cardano tx hash        │     │                                            │
 └──────────────┼─────────────────────────┘     │                                            │
                │                               │                                            │
                │  (2. governance pre-approves  │  add_approved_mc_tx_hashes([hash])         │
                │      that cardano tx hash)  ───────────────►                               │
                │                               │                                            │
                │  3. after 432 Cardano blocks  │  cNIGHT observer sees the locked UTXO,     │
                │     (stability), Midnight     │     emits:                                 │
                │     observes it  ──────────────────────►  • C2MBridge::UserTransfer        │
                │                               │           • DistributeNight(CardanoBridge) │
                │                               │             → credits recipient's          │
                │                               │               claimable balance            │
                │                               │                                            │
                │                               │  4. recipient claims:                      │
                │                               │     ClaimRewards(CardanoBridge)            │
                │                               │       → fresh NIGHT UTXO = amount − fee    │
 └────────────────────────────────────────┘     └────────────────────────────────────────────┘
```

Three things you *do* (steps 1, 2, 4) and one thing you *wait for* (step 3).

---

## Before you start — what you need

| # | Requirement | Notes |
|---|-------------|-------|
| 1 | **The `midnight-node-toolkit` binary** | Build from this repo: `cargo build --release -p midnight-node-toolkit` → `target/release/midnight-node-toolkit`. Or use the published `midnight-node-toolkit` Docker image. All commands below are written as `midnight-node-toolkit …`. |
| 2 | **A Midnight wallet seed** (32-byte hex) | This is your *recipient identity* on Midnight and the thing that later claims. Any 32-byte hex works, e.g. `0000…0001`. Keep it — you need the same seed in steps 1 and 4. |
| 3 | **A Cardano *preview* wallet** with cNIGHT + ADA, and its payment signing-key file | Stagenet follows **Cardano preview**. The wallet must hold some Stagenet cNIGHT (policy `d2dbff…`, empty asset name) plus a little ADA for fees/min-UTXO. The signing key is a standard `*.skey` JSON (`PaymentSigningKeyShelley_ed25519` or the extended variant). Ask the node team how to get preview cNIGHT if you don't have any. |
| 4 | **An Ogmios endpoint** following a Cardano **preview** node | The Cardano-side lock is submitted through Ogmios. Use `wss://ogmios.devnet.midnight.network` — despite the `devnet` name it follows **Cardano preview**, the same chain Stagenet observes, and it's the exact deployment the e2e suite uses (`tests/e2e/src/config.rs`). Note the scheme is **`wss://`** (a WebSocket), not `https://` — the toolkit's Ogmios client is WebSocket-only. |
| 5 | **The Stagenet RPC URL** | The Midnight node you observe and claim against: `wss://rpc.stagenet.shielded.tools`. |
| 6 | **Governance keys for the approval step** *(step 2)* | The approved-tx allow-list is a temporary safety gate (see [`c-to-m-bridge.md`](./c-to-m-bridge.md) → *Unapproved Transfers*). On Stagenet, approving a hash requires **Technical Committee + Council** keys, held by the 7 permissioned validators (TC = validators 1–3, Council = 4–6). **You almost certainly do not hold these** — coordinate with the node team to run step 2, or have them share the keys for a test run. This is the one step you cannot do unilaterally. |
| 7 | **A proof server (optional)** for the claim | The claim (step 4) is a ZK transaction. By default the toolkit proves it in-process with a local prover (needs ZK params available in your environment — `.envrc` wires these in a dev checkout). To offload proving, pass `--proof-server <url>`. |

---

## Stagenet reference values

Everything the bridge is configured with on Stagenet. Sourced from
[`res/stagenet/`](../res/stagenet/) and baked into the genesis chain-spec — **the
bridge is live from block 0; you do not need to run `setMainChainScripts`.**

| Parameter | Value | Source file |
|-----------|-------|-------------|
| Cardano network | **preview** (testnet) | — |
| cNIGHT policy id | `d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68af1` | `cnight-config.json`, `ics-config.json` |
| cNIGHT asset name | *(empty)* | `ics-config.json` |
| **ICS validator address** (lock target) | `addr_test1wrdnz6atrh86np0desq4rfm2vrhrdya6j9zu6n084m9c3eg4tr250` | `ics-config.json` |
| Reserve validator address | `addr_test1wpuq05f3vkyh9jkz6qjsqj6tzsvx7jadk48wktnev8tzkzqk8v6h3` | `reserve-config.json` |
| Bridge fee | **500 basis points (5%)** | `ledger-parameters-config.json` → `cardano_to_midnight_bridge_fee_basis_points` |
| Minimum user transfer | **1000 STAR** | `ledger-parameters-config.json` → `c_to_m_bridge_min_amount` |
| Subminimal flush threshold | 500000 STAR | `c2m-bridge-config.json` |
| Cardano security parameter | **432 blocks** (stability window) | `pc-chain-config.json` |
| Bridge metadata label | `6500973` | (protocol constant) |

> **Units.** Amounts on the wire are **STAR** (the base unit) on both chains — the
> pallet does *not* re-denominate (see
> [`changes/runtime/changed/fix-c2m-bridge-denomination.md`](../changes/runtime/changed/fix-c2m-bridge-denomination.md)).
> `--amount` in `bridge-transfer` and `claim-rewards` is in STAR.

The `--ics-config` file that step 1 needs already exists in the repo:
[`res/stagenet/ics-config.json`](../res/stagenet/ics-config.json).

---

## Step 0 — pin your amounts

Pick a lock amount well above the 1000-STAR minimum so it produces a claimable
User Transfer (rather than being routed to treasury as subminimal). This guide
uses **49,000,000 STAR**, matching the e2e test.

The recipient's claimable amount is the gross lock **minus the 5% fee**:

```
claimable = amount − fee
fee       = 5% of amount           (when amount ≥ 1000 STAR)

49_000_000 − (49_000_000 × 500 / 10_000) = 49_000_000 − 2_450_000 = 46_550_000 STAR
```

(The exact integer arithmetic mirrors `claimable_amount()` in the e2e test.)

---

## Step 1 — derive your recipient address

Your recipient on Midnight is the **raw 32-byte unshielded user address** of your
seed. Get it from the toolkit:

```bash
midnight-node-toolkit show-address \
    --network stagenet \
    --seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --user-address
```

Output is a bare 32-byte hex string, e.g.
`bc610dd07c52f59012a88c2f9f1c5f34cbacc75b868202975d6f19beaf37284b`.
Save it as `RECIPIENT` — it goes into the Cardano metadata in step 1, and the
**same seed** funds the claim in step 4.

---

## Step 2 — lock cNIGHT on Cardano

`bridge-transfer` builds, signs, and submits a Cardano tx that sends your cNIGHT to
the ICS validator address with your recipient address embedded in metadata label
`6500973`:

```bash
midnight-node-toolkit bridge-transfer \
    --signing-key /path/to/your_preview_wallet.skey \
    --ics-config res/stagenet/ics-config.json \
    --recipient-address bc610dd07c52f59012a88c2f9f1c5f34cbacc75b868202975d6f19beaf37284b \
    --amount 49000000 \
    --ogmios-url wss://ogmios.devnet.midnight.network
```

On success it logs the **Cardano tx hash**:

```
Bridge transfer transaction submitted: 9f3c…e21a
```

**Save that hash** — call it `MC_TX_HASH`. It's the identity the bridge tracks,
what governance approves in step 3, and what you match events against.

> **What just happened on Cardano:** one output to the ICS address carrying your
> `--amount` of cNIGHT with a unit inline datum, plus a metadatum under key
> `6500973` holding your 32-byte recipient address. That's the whole "lock". The
> node hasn't seen it yet — Cardano needs to bury it under 432 blocks first
> (step 4's wait).

> **Prefer not to use the toolkit here?** This exact output can be produced with
> `cardano-cli` against your own node socket
> ([`scripts/cnight-generates-dust/lock_to_ics.sh`](../scripts/cnight-generates-dust/lock_to_ics.sh))
> or from a wallet/dApp UI (the
> [demo dApp](https://github.com/midnightntwrk/bridge-demo-dapp/pull/3)). Whatever
> produces the lock, capture its Cardano tx hash and continue with step 3.

---

## Step 3 — pre-approve the Cardano tx hash (governance)

While the tx settles on Cardano, get its hash onto the bridge's approved-tx
allow-list. A hash that isn't approved by the time Midnight observes it is treated
as an **Unapproved Transfer** and swept to the Treasury instead of becoming
claimable — so this must land *before* observation (step 4).

> ⚠️ **This is the step you need the node team for.** `add_approved_mc_tx_hashes`
> is a Root-origin call; on Stagenet that means driving a Council + Technical
> Committee motion with the validators' governance keys. If you don't hold them,
> hand the node team your `MC_TX_HASH` and ask them to run this. The rest of the
> flow (steps 1, 4, 5) is entirely yours.

**3a. Encode the inner call.** The easiest way to get the SCALE-encoded call hex is
[Polkadot-JS Apps](https://polkadot.js.org/apps/) pointed at the Stagenet RPC:
*Developer → Extrinsics →* `c2mBridge.addApprovedMcTxHashes(hashes)`, add one entry
= your `0x<MC_TX_HASH>`, then copy the **"encoded call data"** hex. (You do *not*
submit it here — Polkadot-JS is just the encoder; the call needs Root origin.)

**3b. Execute it through governance** with `root-call`:

```bash
midnight-node-toolkit root-call \
    --rpc-url wss://rpc.stagenet.shielded.tools \
    --council-keys <COUNCIL_KEY_1> <COUNCIL_KEY_2> \
    --tc-keys     <TC_KEY_1> <TC_KEY_2> \
    --encoded-call 0x<encoded-call-hex-from-3a>
```

`root-call` runs the full motion: Council propose + vote + close → Technical
Committee propose + vote + close → federated motion close → the call executes with
Root origin. At least 2 keys from each body are needed for the 2/3 threshold.

> On **local-env** the governance keys are the well-known dev keys
> (Council `//Four //Five //Six`, TC `//One //Two //Three`) — see the e2e test's
> `approve_mc_tx_hash_via_governance`. Stagenet uses the real validator keys.

---

## Step 4 — wait for Midnight to observe it

Midnight only acts on a Cardano tx once it is **stable**: at least
`cardano_security_parameter` = **432 blocks** behind the Cardano tip. On preview
that's a real wait (tens of minutes, longer if preview's block rate is degraded).
Nothing you can do speeds it up — the bridge intentionally waits for finality.

When it lands, the observing block contains:

- a `partnerChainsBridge`/`bridge` `handle_transfers` call carrying a
  `BridgeTransferV1` with your `mc_tx_hash`, `amount`, and recipient;
- a **`C2MBridge::UserTransfer`** event (`mc_tx_hash`, `amount`, `recipient`,
  `midnight_tx_hash`);
- a **`DistributeNight(CardanoBridge, …)`** system transaction crediting the
  recipient's claimable balance.

**How to watch:**

- **Poll your claimable balance** with the toolkit — cleanest signal that the
  credit landed:

  ```bash
  midnight-node-toolkit show-wallet \
      --src-url wss://rpc.stagenet.shielded.tools \
      --seed 0000000000000000000000000000000000000000000000000000000000000001
  ```

  Watch `claimable_bridge_transfers` go from `0` to your post-fee amount
  (`46_550_000`).

- **Or watch events** in Polkadot-JS Apps (*Network → Explorer*, or *chain state*)
  for the `c2mBridge.UserTransfer` event with your `mc_tx_hash`.

- **Or query the indexer** (if you have the Stagenet indexer-api URL): the
  `BridgeUserTransfer` row and `bridgeBalance` reflect the deposit. The e2e test's
  `--features indexer` path documents the exact GraphQL surface
  ([`tests/e2e/README.md`](../tests/e2e/README.md) → *Indexer-side assertions*).

> **If you see `UnapprovedTransfer` instead of `UserTransfer`:** the approval
> (step 3) didn't land before observation. The amount went to the Treasury and is
> not claimable. Re-run with a fresh lock and make sure the hash is approved first.

---

## Step 5 — claim your mNIGHT

Once `claimable_bridge_transfers` shows your amount, claim it with
`generate-txs claim-rewards`, selecting the **`cardano-bridge`** claim kind. Fund it
with the **same seed** whose address you used as the recipient:

```bash
midnight-node-toolkit generate-txs \
    --src-url  wss://rpc.stagenet.shielded.tools \
    --dest-url wss://rpc.stagenet.shielded.tools \
    claim-rewards \
        --funding-seed 0000000000000000000000000000000000000000000000000000000000000001 \
        --amount 46550000 \
        --claim-kind cardano-bridge
```

Notes:

- `--amount` is the **post-fee** claimable (`46_550_000`), not the gross lock.
- `--claim-kind cardano-bridge` is what makes this a bridge claim rather than a
  block-reward claim (the flag defaults to `reward`; see
  [`changes/toolkit/changed/claim-rewards-claim-kind.md`](../changes/toolkit/changed/claim-rewards-claim-kind.md)).
- The claim is self-funded — a fresh recipient with no prior balance can claim
  (this is exactly how local-env's `init-mnight-faucet` bootstraps wallet `00…01`).
- Proving happens in-process by default; add `-p/--proof-server <url>` to offload
  it. To build the tx without submitting, add `--dest-file claim.mn` (and drop
  `--dest-url`) to inspect it first.

---

## Step 6 — verify you were credited

The claim finalizes with a `Midnight::UnshieldedTokens` event whose `created`
UTXOs include a fresh **NIGHT** UTXO (`token_type` all-zeros) at your recipient
address, of value = your claimed amount. Confirm any of:

- `show-wallet` now reports `claimable_bridge_transfers: 0` and your NIGHT balance
  increased by `46_550_000`;
- the indexer's `bridgeBalance` shows `claimed = 46_550_000`, `balance = 0`, and a
  `BridgeClaimTransaction` row for your recipient;
- the `UnshieldedTokens` event in the claim's block (Polkadot-JS Explorer).

That's the happy path complete: cNIGHT locked on Cardano → observed and credited on
Midnight → claimed as mNIGHT. 🎉

---

## Common gotchas

| Symptom | Cause / fix |
|---------|-------------|
| `UnapprovedTransfer` event, nothing claimable | Step 3 approval didn't land before the 432-block observation. Approve the hash *first*, then lock (or re-lock). |
| `InvalidTransfer` → swept to Treasury | The Cardano metadata under label `6500973` wasn't a valid 32-byte address. Use `bridge-transfer` (it encodes correctly); don't hand-build the metadata. |
| Lock amount below 1000 STAR never appears as claimable | It's a *subminimal* transfer — accumulated internally and flushed to Treasury past the threshold, never credited to a recipient. Lock ≥ the minimum (this guide uses 49M). |
| Observation "never" happens | It's the stability wait, not a hang. 432 preview blocks; longer when preview's block rate is degraded. Verify the Cardano tx is actually on-chain and confirm the node is following preview. |
| Claim rejected / can't prove | ZK params not available to the local prover. Point `--proof-server` at a running proof server, or set up params as in `.envrc`. |
| `bridge-transfer` can't balance the tx | The preview wallet lacks enough ADA (min-UTXO + fee) or enough cNIGHT for `--amount`. Fund it. |

---

## Practicing on local-env first (recommended)

Before spending the 432-block preview wait, rehearse the identical flow on the
dockerized **local-env**, where stability is ~5 blocks and the dev governance keys
are yours. Bring it up from [`local-environment/`](../local-environment/):

```bash
cd local-environment
npm run run:local-env            # add :-with-indexer for the indexer surface
```

local-env even performs a real bridge transfer at startup to fund dev wallet
`00…01` (`mint-cnight-supply` → `midnight-setup` pre-approves the hash →
`init-mnight-faucet` claims it) — so the whole happy path runs before you type a
command. See [`local-environment/README.md`](../local-environment/README.md) and
[`changes/node/added/local-network.md`](../changes/node/added/local-network.md).

To run the automated happy-path test against local-env (the canonical recipe):

```bash
cargo test --test e2e_tests --no-default-features --features local,indexer \
    c2m_bridge::bridge_transfer_cnight_to_midnight_address -- --test-threads=1 --nocapture
```

