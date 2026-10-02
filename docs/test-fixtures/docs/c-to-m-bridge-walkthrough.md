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
> (`bridge_transfer_cnight_to_midnight_address`). It **prepares and signs** the
> Cardano lock, **pre-approves its hash** via governance, and only **then submits**
> it — this guide mirrors that ordering. If a command here ever drifts, the test is
> what's real.

> **Why the order matters — the lock is prepared before it is submitted.** The
> approved-tx allow-list is a safety gate: a Cardano tx hash that isn't approved by
> the time Midnight observes it is swept to the Treasury as an *Unapproved Transfer*
> instead of becoming claimable (see [`c-to-m-bridge.md`](./c-to-m-bridge.md) →
> *Unapproved Transfers*). To approve a hash you first have to *know* it, and a
> Cardano tx hash is fixed the moment the tx **body** is built — signing and
> submitting don't change it. So the clean flow is: **build the lock (don't submit)
> → read its hash → pre-approve it → sign & submit.**
>
> This guide builds the lock with **`cardano-cli`**, because that lets you read the
> tx hash off the unsigned body *before* submitting. Everything on the Cardano side
> is ordinary tx construction: send cNIGHT to the ICS address with the recipient in
> metadata label `6500973`. The worked recipe is
> [`scripts/cnight-generates-dust/lock_to_ics.sh`](../scripts/cnight-generates-dust/lock_to_ics.sh)
> (it builds and signs, prints the txid, and deliberately stops short of submitting
> — *"add to allowlist first"*).
>
> **Any tool that separates signing from submission works the same way.** The demo
> dApp ([midnightntwrk/bridge-demo-dapp#3](https://github.com/midnightntwrk/bridge-demo-dapp/pull/3),
> [demo video](https://drive.google.com/file/d/1uw0jD8INJexyoievtYOYhKV1zY-pfOld/view))
> keeps signing and submission as distinct steps, so you can capture the tx hash,
> get it approved, and only then submit from the UI — the same prepare → approve →
> submit ordering this guide drives with `cardano-cli`.
>
> The one exception is the toolkit's `bridge-transfer` command, which
> builds+signs+**submits in a single call** and prints the hash only *afterwards*,
> so it can't feed the approval ahead of time. It still works *if* the hash is
> approved before observation (~432 blocks), but it races that window — see the note
> at the end of [Step 4](#step-4--sign--submit-the-lock-tx).
>
> The Midnight side (approval, observation, claim) is the same regardless of how
> the Cardano lock was produced.

---

## The happy path at a glance

A *User Transfer* is a Cardano wallet locking cNIGHT at the ICS and a Midnight
address later claiming the equivalent mNIGHT. Steps **1** and **3** happen on
Cardano; steps **2**, **4**, and **5** on Midnight:

```
  CARDANO (Preview)                         MIDNIGHT (Stagenet)
  ─────────────────                         ───────────────────

 ┌───────────────────────────────┐
 │ 1. PREPARE the lock tx        │
 │    (build, don't submit):     │
 │    cNIGHT → ICS address,      │
 │    recipient in tx metadata   │
 │    (label 6500973).           │
 │    Read the Cardano tx hash.  │
 └───────┬───────────────────────┘
         │                                ┌────────────────────────────────────┐
         └─ tx hash ─────────────────────►│ 2. PRE-APPROVE that hash,          │
                                          │    before the lock is              │
                                          │    submitted:                      │
                                          │    governance →                    │
                                          │    add_approved_mc_tx_hashes       │
                                          │    ([hash])                        │
                                          └───────┬────────────────────────────┘
 ┌───────────────────────────────┐                │
 │ 3. SIGN & SUBMIT the lock     │◄───────────────┘
 │    tx to Cardano.             │
 └───────┬───────────────────────┘
         │                                ┌────────────────────────────────────┐
         └─ after 432 blocks ────────────►│ 4. OBSERVE — Midnight sees the     │
                                          │    locked UTXO, emits:             │
                                          │    • C2MBridge::UserTransfer       │
                                          │    • DistributeNight(CardanoBridge)│
                                          │      → credits claimable balance   │
                                          └─────────────────┬──────────────────┘
                                                            ▼
                                          ┌─────────────────┴──────────────────┐
                                          │ 5. CLAIM — recipient calls         │
                                          │    ClaimRewards(CardanoBridge)     │
                                          │    → fresh NIGHT UTXO              │
                                          │      = amount − fee                │
                                          └────────────────────────────────────┘
```

Four things you *do* (steps 1, 2, 3, 5) and one thing you *wait for* (step 4).

---

## Before you start — what you need

| # | Requirement | Notes |
|---|-------------|-------|
| 1 | **The `midnight-node-toolkit` binary** | Build from this repo: `cargo build --release -p midnight-node-toolkit` → `target/release/midnight-node-toolkit`. Or use the published `midnight-node-toolkit` Docker image. Used here for `show-address`, `root-call`, `show-wallet`, and the claim. **⚠️ Build from `main` (or at least commit `fd0ae836`, #1766) — the claimable-balance fields `show-wallet` reports in Steps 5–6 were added there and are *not* on `release/node-2.0.0`.** |
| 2 | **A Midnight wallet seed** (32-byte hex) | This is your *recipient identity* on Midnight and the thing that later claims. Any 32-byte hex works, e.g. `0000…0001`. Keep it — you need the same seed in Step 1 (derive) and Step 6 (claim). |
| 3 | **A Cardano *Preview* wallet** with cNIGHT + ADA, and its payment signing-key file | Stagenet follows **Cardano Preview**. The wallet must hold some Stagenet cNIGHT (policy `d2dbff…`, empty asset name) plus a little ADA for fees/min-UTXO. The signing key is a standard `*.skey` JSON. Get cNIGHT from the [Midnight faucet](https://midnight-faucet.nethermind.dev/) and Preview ADA from the [Cardano testnets faucet](https://docs.cardano.org/cardano-testnets/tools/faucet). |
| 4 | **`cardano-cli` + a Cardano *Preview* node socket** | You build, sign, and submit the lock with `cardano-cli`, which needs `CARDANO_NODE_SOCKET_PATH` pointed at a running **Preview** node (that's what lets you read the tx hash *before* submitting — the whole point of the ordering). *If you instead use the one-shot toolkit `bridge-transfer`, you need an Ogmios endpoint following Preview — `wss://ogmios.devnet.midnight.network` (WebSocket, `wss://`) — rather than a node socket.* |
| 5 | **The Stagenet RPC URL** | The Midnight node you approve, observe, and claim against: `wss://rpc.stagenet.shielded.tools`. |
| 6 | **Governance keys for the approval step** *(Step 3)* | The approved-tx allow-list is a temporary safety gate (see [`c-to-m-bridge.md`](./c-to-m-bridge.md) → *Unapproved Transfers*). On Stagenet, approving a hash requires **Technical Committee + Council** keys, held by the 7 permissioned validators (TC = validators 1–3, Council = 4–6). **You almost certainly do not hold these** — coordinate with the node team to run Step 3, or have them share the keys for a test run. This is the one step you cannot do unilaterally. |
| 7 | **A proof server (optional)** for the claim | The claim (Step 6) is a ZK transaction. By default the toolkit proves it in-process with a local prover (needs ZK params available in your environment — `.envrc` wires these in a dev checkout). To offload proving, pass `--proof-server <url>`. |

---

## Stagenet reference values

Everything the bridge is configured with on Stagenet. Sourced from
[`res/stagenet/`](../res/stagenet/) and baked into the genesis chain-spec.

> ⚠️ **The bridge is *disabled* at Stagenet genesis** — deliberately, so Stagenet
> can rehearse the live-chain enable procedure that mainnet and other non-reset
> chains require. **Before you lock anything, confirm it's on**: `bridge.dataCheckpoint`
> must be non-empty. How to check and how to switch it on is in
> [Enabling the C-to-M bridge](./c-to-m-bridge-enabling.md).

| Parameter | Value | Source file |
|-----------|-------|-------------|
| Cardano network | **Preview** (testnet) | — |
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
> Amounts in the commands below are in STAR.

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
Save it as `RECIPIENT` — it goes into the Cardano metadata in step 2, and the
**same seed** funds the claim in step 5.

---

## Step 2 — prepare the Cardano lock tx (and read its hash)

Build — but **do not submit** — the Cardano tx that locks your cNIGHT at the ICS
validator address, with your recipient address embedded in metadata label
`6500973`. Building it is enough to fix its **Cardano tx hash**, which is what you
pre-approve in step 3. The full worked recipe is
[`scripts/cnight-generates-dust/lock_to_ics.sh`](../scripts/cnight-generates-dust/lock_to_ics.sh);
the essentials:

**2a. Write the bridge metadatum** (`metadata.json`) — a single-element list
holding your 32-byte recipient address as bytes, under the bridge label:

```json
{
  "6500973": {
    "list": [
      { "bytes": "bc610dd07c52f59012a88c2f9f1c5f34cbacc75b868202975d6f19beaf37284b" }
    ]
  }
}
```

**2b. Build the tx body** (needs `CARDANO_NODE_SOCKET_PATH` on a **Preview** node).
One output sends your `--amount` of cNIGHT to the ICS address with an inline unit
datum; change goes back to your wallet:

```bash
cardano-cli conway transaction build \
    --tx-in <your-cnight-utxo> \
    --tx-in <your-fee-utxo> \
    --tx-in-collateral <your-fee-utxo> \
    --tx-out "addr_test1wrdnz6atrh86np0desq4rfm2vrhrdya6j9zu6n084m9c3eg4tr250+1500000 lovelace + 49000000 d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68af1" \
    --tx-out-inline-datum-value '{"constructor": 0, "fields": []}' \
    --metadata-json-file metadata.json \
    --json-metadata-detailed-schema \
    --change-address <your-preview-wallet-address> \
    --out-file lock-to-ics.tx
```

(cNIGHT has an *empty* asset name, so the value is just `<amount> <policy-id>` with
no `.assetname` suffix.)

**2c. Read the tx hash off the unsigned body:**

```bash
cardano-cli conway transaction txid --tx-body-file lock-to-ics.tx
```

**Save that hash** — call it `MC_TX_HASH`. It's the identity the bridge tracks:
what governance approves in step 3, what you sign & submit in step 4, and what you
match events against. **Do not submit the tx yet.**

> **What this lock is:** one output to the ICS address carrying your cNIGHT with a
> unit inline datum, plus a metadatum under key `6500973` holding your 32-byte
> recipient address. Signing (step 4) adds a witness but does **not** change the
> hash, so the hash you read here is final.

---

## Step 3 — pre-approve the Cardano tx hash (governance)

Get `MC_TX_HASH` onto the bridge's approved-tx allow-list *before* the lock is
submitted. A hash that isn't approved by the time Midnight observes it is treated
as an **Unapproved Transfer** and swept to the Treasury instead of becoming
claimable — approving first (while the tx is still only *prepared*) removes any
race against the observation window.

> ⚠️ **This is the step you need the node team for.** `add_approved_mc_tx_hashes`
> is a Root-origin call; on Stagenet that means driving a Council + Technical
> Committee motion with the validators' governance keys. If you don't hold them,
> hand the node team your `MC_TX_HASH` and ask them to run this. The rest of the
> flow (steps 1, 2, 4, 5) is entirely yours.

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

## Step 4 — sign & submit the lock tx

With the hash approved, sign the tx you built in step 2 and submit it to Cardano:

```bash
cardano-cli conway transaction sign \
    --tx-body-file lock-to-ics.tx \
    --signing-key-file /path/to/your_preview_wallet.skey \
    --out-file lock-to-ics-signed.tx

cardano-cli conway transaction submit \
    --tx-file lock-to-ics-signed.tx
```

`submit` returns once the tx is accepted into the mempool. The hash is unchanged
from step 2c — you can re-confirm with
`cardano-cli conway transaction txid --tx-file lock-to-ics-signed.tx`. That's the
whole "lock" on Cardano; the node hasn't seen it yet — Cardano needs to bury it
under 432 blocks first (step 5's wait).

> **One-shot alternative (toolkit `bridge-transfer`).** If you'd rather not touch
> `cardano-cli`, the toolkit builds+signs+submits in a single call:
>
> ```bash
> midnight-node-toolkit bridge-transfer \
>     --signing-key /path/to/your_preview_wallet.skey \
>     --ics-config res/stagenet/ics-config.json \
>     --recipient-address bc610dd07c52f59012a88c2f9f1c5f34cbacc75b868202975d6f19beaf37284b \
>     --amount 49000000 \
>     --ogmios-url wss://ogmios.devnet.midnight.network
> ```
>
> It logs the hash (`Bridge transfer transaction submitted: 9f3c…e21a`) — but only
> **after** submitting, so it can't feed step 3 ahead of time. Using it inverts the
> order to *submit → approve*, which works **only if** the approval lands before the
> 432-block observation. Coordinate the approval first, and treat the window as a
> deadline. The `--ics-config` file already exists at
> [`res/stagenet/ics-config.json`](../res/stagenet/ics-config.json).

---

## Step 5 — wait for Midnight to observe it

Midnight only acts on a Cardano tx once it is **stable**: at least
`cardano_security_parameter` = **432 blocks** behind the Cardano tip. On Preview
that's a real wait (tens of minutes, longer if Preview's block rate is degraded).
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
  (`46_550_000`). If `show-wallet` doesn't print this field, your toolkit predates
  `fd0ae836` (#1766) — rebuild from `main` (see requirement 1).

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

## Step 6 — claim your mNIGHT

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

## Step 7 — verify you were credited

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
| `UnapprovedTransfer` event, nothing claimable | The hash wasn't approved before the 432-block observation. Approve the hash first (step 3), *then* sign & submit (step 4). If you used the one-shot toolkit path, the submit beat the approval — re-lock and approve first. |
| `InvalidTransfer` → swept to Treasury | The Cardano metadata under label `6500973` wasn't a valid 32-byte address. Follow the `metadata.json` shape in step 2a exactly (a `list` with one `bytes` entry); don't hand-build a different structure. |
| Lock amount below 1000 STAR never appears as claimable | It's a *subminimal* transfer — accumulated internally and flushed to Treasury past the threshold, never credited to a recipient. Lock ≥ the minimum (this guide uses 49M). |
| Observation "never" happens | **First rule out a disabled bridge:** query `bridge.dataCheckpoint` (see [Enabling the C-to-M bridge](./c-to-m-bridge-enabling.md)) — if it's empty, the bridge is off and nothing will ever be observed until it's enabled via governance. If it's set, it's just the stability wait, not a hang: 432 Preview blocks, longer when Preview's block rate is degraded. Verify the Cardano tx is actually on-chain and the node is following Preview. |
| Claim rejected / can't prove | ZK params not available to the local prover. Point `--proof-server` at a running proof server, or set up params as in `.envrc`. |
| `cardano-cli transaction build` can't balance the tx | The Preview wallet lacks enough ADA (min-UTXO + fee) or enough cNIGHT for the `--tx-out` amount. Fund it, and double-check the `--tx-in` UTXOs. |

---

## Practicing on local-env first (recommended)

Before spending the 432-block Preview wait, rehearse the identical flow on the
dockerized **local-env**, where stability is ~5 blocks and the dev governance keys
are yours. Bring it up from [`local-environment/`](../local-environment/):

```bash
cd local-environment
npm run run:local-env            # add :-with-indexer for the indexer surface
```

Because the flow is *prepare → pre-approve → sign & submit → observe → claim*, a
"rehearsal" isn't a single re-run of one command — you still build the Cardano tx,
get its hash approved, and only then submit and claim. What local-env buys you is
the short stability window and the dev governance keys, so you can drive those
separate steps (2–6) yourself in minutes.

local-env's own startup funds dev wallet `00…01` through the bridge, but via a
*genesis* shortcut rather than the live governance flow: `mint-cnight-supply`
submits the funding lock on Cardano pre-genesis, then `midnight-setup` bakes that
tx hash straight into the c2m-bridge genesis `approved_txs` (so it's approved from
block 0, no governance round), and `init-mnight-faucet` claims it once blocks are
producing. So the whole happy path runs before you type a command. See
[`local-environment/README.md`](../local-environment/README.md) and
[`changes/node/added/local-network.md`](../changes/node/added/local-network.md).

To run the automated happy-path test against local-env (the canonical recipe):

```bash
cargo test --test e2e_tests --no-default-features --features local,indexer \
    c2m_bridge::bridge_transfer_cnight_to_midnight_address -- --test-threads=1 --nocapture
```
