# Enabling the C-to-M bridge

How to tell whether the **Cardano-to-Midnight (C2M) bridge** is switched on for a
chain, and how to switch it on via governance.

> **Context.** For the concepts read [`c-to-m-bridge.md`](./c-to-m-bridge.md); to
> actually move tokens once the bridge is on, see the
> [Happy Path walkthrough](./c-to-m-bridge-walkthrough.md). This doc is only about
> the on/off switch.

## Why a bridge can be "off"

The bridge observer only runs once it has an **initial data checkpoint** — a point
on Cardano to start reading from. That checkpoint can be set two ways:

- **Baked into genesis** (`initial_checkpoint` in the `Bridge` pallet's genesis
  config). `dev` and local-env chains do this, so the bridge is live from block 0.
- **Set on a live chain** via a governance call. Networks that must not be reset —
  **mainnet**, and testnets like **Stagenet** that deliberately rehearse the
  mainnet procedure — ship with `initial_checkpoint` **unset** and are switched on
  later with `bridge.setMainChainScripts`.

Until the checkpoint exists, cNIGHT locked on Cardano is never observed and never
becomes claimable — the lock just sits there.

## Is the bridge enabled? (check)

The on/off switch is the `Bridge` pallet's **`dataCheckpoint`** storage, an
`Option`: when it holds a value the observer knows where to start; when it's
**empty, the bridge is disabled**.

Check it in Polkadot-JS Apps → *Developer → Chain state* → query
`bridge.dataCheckpoint()`. Point the UI at the target chain's RPC, e.g. Stagenet:
[chain state on Stagenet](https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Frpc.stagenet.shielded.tools#/chainstate).

- **Empty / `None`** → the bridge is **disabled**. Nothing locked will be observed.
- **A `Tx(0x…)` or `Block(n)` value** → the bridge is **live**.

## Enabling it (governance)

Switching the bridge on is a single Root-origin extrinsic:

```
bridge.setMainChainScripts(newScripts, dataCheckpoint)
```

It writes both the Cardano scripts to observe **and** the initial checkpoint, in one
call. It must run with Root origin — on a permissioned chain that means a
Council + Technical Committee governance motion, not a direct submission.

**`newScripts`** (`MainChainScripts`) — the Cardano side to observe. Its four fields
come straight from the chain's `res/<network>/` config:

| Field | Value source (`res/<network>/`) |
|-------|---------------------------------|
| `token_policy_id` | cNIGHT minting policy id — `cnight-config.json` / `ics-config.json` |
| `token_asset_name` | cNIGHT asset name (empty for cNIGHT) — `ics-config.json` |
| `illiquid_circulation_supply_validator_address` | ICS validator address (the lock target) — `ics-config.json` |
| `reserve_validator_address` | Reserve validator address — `reserve-config.json` |

(For Stagenet these are the exact values in the walkthrough's
[Stagenet reference values](./c-to-m-bridge-walkthrough.md#stagenet-reference-values).)

**`dataCheckpoint`** (`BridgeDataCheckpoint`) — where the observer starts reading
Cardano. One of:

- `Tx(<mc_tx_hash>)` — start just after a specific Cardano tx. The **chain genesis
  UTXO** is the usual choice, so observation begins from the chain's own start.
- `Block(<mc_block_number>)` — start from a Cardano block number.

Pick a checkpoint at or before the first bridge transfer you want observed; anything
locked *before* the checkpoint is not seen.

### Running the call

Same mechanics as the approval step in the walkthrough
([Step 3](./c-to-m-bridge-walkthrough.md#step-3--pre-approve-the-cardano-tx-hash-governance)):

1. **Encode the call.** In [Polkadot-JS Apps](https://polkadot.js.org/apps/) →
   *Developer → Extrinsics* → `bridge.setMainChainScripts(newScripts, dataCheckpoint)`,
   fill in the fields, and copy the **"encoded call data"** hex. (Don't submit it
   here — it needs Root origin.)
2. **Drive it through governance** with `root-call`, supplying at least 2 keys from
   each body (2/3 threshold):

   ```bash
   midnight-node-toolkit root-call \
       --rpc-url wss://rpc.stagenet.shielded.tools \
       --council-keys <COUNCIL_KEY_1> <COUNCIL_KEY_2> \
       --tc-keys     <TC_KEY_1> <TC_KEY_2> \
       --encoded-call 0x<encoded-call-hex>
   ```

   `root-call` runs the full motion (Council propose + vote + close → Technical
   Committee propose + vote + close → federated motion close → Root execution).

3. **Verify.** Re-query `bridge.dataCheckpoint()` (above) — it should now hold your
   `Tx`/`Block` value. The observer picks up from the next block; from here the
   [Happy Path walkthrough](./c-to-m-bridge-walkthrough.md) applies.

> On **local-env** the governance keys are the well-known dev keys
> (Council `//Four //Five //Six`, TC `//One //Two //Three`). A live chain uses its
> real validator keys, which you almost certainly do not hold — coordinate with the
> node team to run this.
