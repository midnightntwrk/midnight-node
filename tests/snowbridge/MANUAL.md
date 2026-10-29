# Watch and operate the local bridge in Polkadot.js

This is the interactive companion to the [automated runbook](README.md#local-end-to-end-runbook).
It uses the same proofs, calls and assertions as the successful live PoC, with
operator-controlled pauses. You can submit individual Midnight calls in
Polkadot.js, alternate with terminal-driven calls, and inspect finalized state
between steps. Ethereum deployment/release and proof generation use the existing
terminal helpers. A read-only local viewer decodes Compact and Ethereum state.

The automated bidirectional run is validated. On 2026-10-24 the manual gate,
canonical finalized-event attribution, wrong-call/wrong-signer rejection and
read-only viewer were tested against a disposable live Midnight node
(`artifacts/snowbridge/manual-control-smoke.log`); the eight JavaScript tests also
passed. That smoke test used a separate API submission to exercise the UI
acknowledgement path; it did not click through a browser or run a second full
bidirectional transfer. An entire operator-driven browser walkthrough is a
separate run, not the previous automated result. The tools do not mark your
walkthrough successful until its own final audit passes.

## 1. Prepare once, then start a paced run

Complete the prerequisites and `run.sh` / `node-test.sh` steps in the automated
runbook first. Use Node 22+ (validated with 24.14.0). Keep Docker and the host awake.
In terminal A, at the repository root:

```bash
SNOWBRIDGE_MANUAL=1 python3 tests/snowbridge/pos-test.py
```

Copy the exact `Owned Ethereum PoS stack: .../run-...` directory printed during
startup. In terminal B set it once, using your actual path:

```bash
export BRIDGE_RUN="$PWD/artifacts/snowbridge/ethereum-pos/run-REPLACE"
node tests/snowbridge/manual.mjs status "$BRIDGE_RUN"
```

Genesis/proof setup takes time before the first pause. Once the node is ready,
the first pending step is **Local chains ready**. Both chains continue producing
blocks while the bridge driver waits. Pending steps and call inputs are in
`$BRIDGE_RUN/manual/pending.json`; all requests and responses are retained there.
The detailed driver log is `$BRIDGE_RUN/midnight-live.log`.

**Pauses do not stop blockchain time.** The existing Compact/DUST intents expire
one hour after their creation; submit the escrow and all five Compact transactions
within one hour of genesis preparation. Do not leave the walkthrough overnight.
Long pauses can also cross BEEFY authority-set boundaries beyond this PoC's
tested relay path. If that happens, preserve the failed run and start fresh.
The final inspection pause, after all transactions, is the place to spend longer
reviewing history. No timeout bypass or proof-validation relaxation is enabled.

## 2. Open the two views

Polkadot.js Apps provides block/extrinsic/event inspection, storage queries and
transaction submission. Its [upstream instructions](https://github.com/polkadot-js/apps#docker)
support a local Docker UI. In terminal C:

```bash
docker run --rm --name midnight-bridge-ui \
  -e WS_URL=ws://127.0.0.1:19944 \
  -p 127.0.0.1:18081:80 jacogr/polkadot-js-apps:latest
```

Open <http://127.0.0.1:18081/?rpc=ws%3A%2F%2F127.0.0.1%3A19944#/explorer>.
The browser connects to your host's WebSocket endpoint; the bridge node is not
inside the UI container. The UI image is an optional upstream tool, not part of
the pinned consensus/proving stack. Its exact labels may vary by release.
If using another Apps installation, select the custom endpoint
`ws://127.0.0.1:19944`. Prefer the local HTTP UI to avoid HTTPS-to-local-WebSocket
restrictions. The hosted application is optional; no hosted service is needed.

In terminal D, with the same `BRIDGE_RUN` value:

```bash
node tests/snowbridge/manual.mjs serve "$BRIDGE_RUN"
```

Open <http://127.0.0.1:18080>. This viewer refreshes every three seconds and shows:

- The pending manual step and exact call arguments.
- Midnight deposit records, redemption phases, nonce bindings and halt state.
- Decoded Compact balances, retirement records and operation names.
- Ethereum custody/recipient balances at both latest and finalized execution blocks.
- Recent finalized calls, dispatch events, ledger statistics and imported beacon root.

It has no transaction buttons or write API. Missing contracts before deployment
are shown as unavailable, not as a successful zero balance. Ethereum bootstrap
or connection errors are shown explicitly. All Midnight values in a snapshot
come from the displayed finalized block. Ethereum latest can change before its
finalized values do; this is expected during finality waits.

For custom ports, export the same `SNOWBRIDGE_NODE_PORT` in the orchestrator and
viewer terminals and change the Apps endpoint. `SNOWBRIDGE_VIEW_PORT` changes
18080. See the automated runbook for Ethereum/P2P overrides.

## 3. Advance one step at a time

Run `status` or read the viewer's pending-step panel. Each step has a unique `id`.
Copy it into the following commands; the ID prevents a stale command advancing
a later step by mistake.

For a helper or inspection pause, or to let the terminal submit a Midnight call:

```bash
node tests/snowbridge/manual.mjs next "$BRIDGE_RUN" STEP_ID
```

That releases exactly the current gate. For a helper it runs the displayed
command/arguments/environment; it does not skip proof construction. Some helpers
wait for Ethereum finality and have no immediate output. Let them finish.
The ingress-preparation helper deploys the Ethereum contracts and deposits 100
as one stage; you can watch the resulting Ethereum state while it waits for
finality. Do not run a helper separately and then press `next`: that would run it twice.

### Submit a Midnight call yourself in Polkadot.js

At a pending step with `kind: "extrinsic"`:

1. In **Developer → Extrinsics**, choose the displayed pallet and call, and fill
   the displayed arguments. Use the exact signer in `pending.json` (public key
   is also shown). Add the appropriate public development account, such as
   `//Alice`, in Accounts if necessary. These are disposable development keys.
2. Submit the call and find it in **Network → Explorer**. Wait for its block to
   finalize. Copy the inclusion **block hash** and the **extrinsic hash**; a block
   number or call hash is not a substitute.
3. Acknowledge that exact submission in terminal B:

   ```bash
   node tests/snowbridge/manual.mjs submitted "$BRIDGE_RUN" STEP_ID BLOCK_HASH EXTRINSIC_HASH
   ```

The driver reads canonical finalized history and checks the call bytes, signer,
signed/unsigned form and that extrinsic's own events. Merely acknowledging a
different transaction cannot satisfy the step. A mismatched acknowledgement fails
the run. After a UI submission, **do not use `next` for that same step**, because
`next` would submit it again.

The easiest calls to perform manually are `materializeDeposit`,
`exportRedemption` and `retryRedemption`. They take a single ID or sequence.
The pending JSON also contains SCALE call/extrinsic hex and structured arguments
for larger proof calls. Use terminal submission for these unless you want to
enter the complete nested proof in the UI. If your Apps build cannot sign the
runtime's custom signed extensions, use terminal submission and inspect the same
finalized call in Explorer; do not change the signed payload manually.

`midnight.sendMnTransaction` carries an already proved/signed ledger transaction
and is submitted as an **unsigned Substrate extrinsic** (`signer: null` in the
pending file). Its embedded ledger signatures and proofs still apply. Do not
select Alice as its outer signer. Use terminal submission if your UI does not
offer the unsigned option. Compact asset ownership is independent of the Alice
Substrate account; the ordinary Balances/Accounts screen is not its token balance.

## 4. Follow the state transitions

Use **Developer → Chain state → Storage** to add the queries below. The
[Polkadot developer-interface documentation](https://wiki.polkadot.com/general/polkadotjs-ui/)
describes these screens. Use the displayed finalized block hash when comparing
with the local viewer; an unpinned storage subscription may show the newer best head.

| Stage / pending call | What to inspect | Expected result |
| --- | --- | --- |
| Local chains ready | Explorer; `ethereumBridge.activeRegistry`, `deposits`, `redemptions` | Blocks finalize; bridge is inactive and maps are empty. |
| `ingress-preparation` helper | Viewer Ethereum deployment, custody and block heights | Contracts deploy; custody becomes 100, then the deposit finalizes. |
| Governance bootstrap | `council` / `technicalCommittee` events; `ethereumBeaconClient.latestFinalizedBlockRoot` | Two bodies approve the checkpoint; imported root becomes nonzero. |
| `ethereumBeaconClient.submit` | Imported beacon root; successful extrinsic event | A real finalized update is verified. |
| Escrow `midnight.sendMnTransaction` | `ContractDeploy` / `TxApplied`; viewer escrow | Escrow exists with zero balance and sole operation `retire`. |
| Governance activation | `ethereumBridge.activeRegistry`; `Activated` event | Frozen asset, gateway and escrow configuration appears. |
| `ethereumBridge.submitDeposit` | `Pending` event; `deposits(DEPOSIT_ID)` | Credit amount 100 and `issued: false`. Copy the ID from the event or viewer. |
| **Manually submit `materializeDeposit(DEPOSIT_ID)`** | `Issued`, `midnightSystem.SystemTransactionApplied`; deposit record | `issued: true`; a native UTXO is created. Ethereum custody stays 100. |
| Duplicate deposit/issuance calls | `system.ExtrinsicFailed` | `AlreadySeen` / `AlreadyIssued`; the driver intentionally tests these failures. |
| Compact 0: deploy consumer | `TxApplied`; viewer consumer | Consumer exists, balance 0. |
| Compact 1: deposit 100 | `TxApplied`; consumer balance | Consumer 100. |
| Compact 2: query balance | `TxApplied`; consumer balance | Still 100; this is an ordinary proved contract call. |
| Compact 3: withdraw 40 | `TxApplied`; consumer balance | Consumer 60; 40 temporarily belongs to the fixture's signed wallet. |
| Compact 4: retire 40 | `TxApplied`; viewer escrow | Escrow 40, `nextSequence: 1`, record 0 amount 40 and recipient `0x0505…0505`. |
| **Manually submit `exportRedemption(0)`** | `RedemptionQueued`, `RedemptionAccepted`; `redemptions(ID)`, `byNonce(NONCE)` | Stable redemption ID, attempt 0, then `AwaitingResult` with nonce 1 on a fresh run. |
| Duplicate export / premature retry | Failed extrinsic events | `AlreadyExported` / `NotRetryEligible`. |
| `mmr-convert-0`, `release-0` helpers | Proof JSON, viewer Ethereum release logs | Actual consensus proof verifies; intended token release fails, custody 100 / recipient 0. The outer Ethereum transaction succeeds while its dispatch-result event is false. |
| `release-receipt-0`, beacon update, `submitRedemptionReceipt` | `RedemptionResult(success: false)`; redemption phase | After finality and receipt verification, phase becomes `RetryEligible`. |
| **Manually submit `retryRedemption(ID)`** | `RedemptionAccepted`; `redemptions`, `byNonce` | Attempt 1, fresh nonce 2, same amount/recipient/ID. |
| `mmr-convert-1`, `release-1` | Viewer latest then finalized Ethereum balances | Custody 60 / recipient 40. Midnight still awaits the authenticated result. |
| `release-receipt-1`, beacon update, `submitRedemptionReceipt` | `RedemptionResult(success: true)`; redemption phase | `Completed`. |
| Duplicate/stale receipts and retry after completion | Failed extrinsic events | `AlreadySeen` / `NotRetryEligible`; balances do not change. |
| Final inspection pause | Both UIs and canonical event history | Consumer 60, sealed retired 40, Ethereum custody 60, recipient 40; redemption completed. |

`Queued` can be very brief because the runtime queue processes independently of
the paused driver. Inspect its event history if the storage query already shows
`AwaitingResult`. Outbound commitment roots/messages are likewise transient;
`byNonce` and the saved historical MMR proofs retain the relevant binding.
Governance is not a sudo shortcut: the driver exposes each proposal/vote/close
step for the council and technical committee, then the federated motion close.
Use the displayed signer for each, or let the terminal execute those setup calls.

## 5. Finish or stop

At **Flow assertions passed: inspect final states**, both chains are still up.
Review the UIs for as long as needed. Then `next` runs the independent issuance
replay and final Ethereum audit and shuts down the owned chains. Success still
requires exit code zero and this run's `validation.json` with
`completeBridgeE2E: true`, not merely a green transaction in a UI.

To stop at a pending gate:

```bash
node tests/snowbridge/manual.mjs abort "$BRIDGE_RUN" STEP_ID
```

That fails the current run and lets the orchestrator clean up. Stop the read-only
viewer and the UI container with Ctrl-C in their own terminals. The viewer will
show disconnection after chain teardown; it is not a persisted historical indexer.
For a hard interruption, use the run-specific cleanup in the automated runbook.
There is no resume-from-arbitrary-step facility: saved responses are an audit
trail, not authorization to replay a partially completed walkthrough.
