<!-- markdownlint-disable MD013 MD060 -->
# Ledger 9 and the Cardano→Midnight bridge (2.0.0-alpha.1)

This release ships two headline changes.

> **Breaking:** Ledger 9 has no migration path from ledger 8. Any existing chain must be abandoned and a fresh chain started.
>
> **Inert feature:** The Cardano→Midnight (C2M) bridge is compiled in but **not enabled in any environment** in this alpha release. Enabling it requires governance actions that have not yet been executed.

---

## Ledger 9

### What changed

The active ledger is now `midnight-ledger-v9` (`mn-ledger-9 = { version = "=0.1.0", package = "midnight-ledger-v9" }`, pinned to tag `crate-ledger-9.0.1.0-alpha.1`). The `types::active_ledger_bridge` and `latest` aliases in `midnight-node-ledger` both point to ledger 9. Ledger 7 and 8 module definitions are retained for historical block replay but are no longer the active path.

Companion crate bumps included in this release:

| Alias | Tag/version |
| ----- | ----------- |
| `mn-ledger-9` | `=0.1.0` (midnight-ledger-v9) |
| `midnight-zswap` | `zswap-9.0.0-alpha.1` |
| `midnight-onchain-vm` | `onchain-vm-4.0.0-alpha.1` |
| `midnight-onchain-state` | `onchain-state-4.0.0-alpha.1` |
| `midnight-onchain-runtime` | `onchain-runtime-4.0.0-alpha.1` |

### What the Ledger 9 upgrade unlocks (vs ledger 8.1)

Ledger 8.1.0 — the version node 1.0.0 pinned — was a *minor* release: storage-core stability fixes (race/deadlock/memory) plus finer-grained wallet WASM bindings, with no new ledger semantics. The jump to ledger 9 is what supplies the primitives node 2.0.0 is built around. Sources: the [ledger 9.0.1.0-alpha.1 changelog](https://github.com/midnightntwrk/midnight-ledger/blob/crate-ledger-9.0.1.0-alpha.1/CHANGELOG.md) and the [ledger 8.1.0 release notes](https://github.com/midnightntwrk/midnight-ledger/releases/tag/ledger-8.1.0).

| Change | Why it matters | Ledger PR |
| ------ | -------------- | --------- |
| `UnlockToTreasury` system transaction (locked pool → treasury) | Powers the C2M bridge invalid/unapproved-transfer → treasury path; replaces the removed v8 `construct_distribute_treasury_system_tx` | [#505](https://github.com/midnightntwrk/midnight-ledger/pull/505), tests [#534](https://github.com/midnightntwrk/midnight-ledger/pull/534) |
| ECDSA signature support | The secp256k1/ECDSA primitive the Cardano→Midnight bridge needs | [#498](https://github.com/midnightntwrk/midnight-ledger/pull/498) (base-crypto [#252](https://github.com/midnightntwrk/midnight-ledger/pull/252)) |
| Explicit price floor, denominated in full blocks, governed by ledger parameters | A governed minimum on block-fullness fee pricing | [#463](https://github.com/midnightntwrk/midnight-ledger/pull/463) |
| Correctly exclude the identity point during coin ciphertext decryption | Cryptographic correctness fix (audit issue C — `is_infinity` was meant to check the identity element; a hard fork on tx validation) | [#464](https://github.com/midnightntwrk/midnight-ledger/pull/464) |
| Split-phase execution: `apply_guaranteed_only` + `GuaranteedApplyResult` (deferred event generation) | Apply the guaranteed phase independently of the fallible phase; not yet consumed by the node | [#309](https://github.com/midnightntwrk/midnight-ledger/pull/309) |
| proof-server support for ZKIR 2.1 | Newer ZK intermediate representation | see changelog |
| Fix: potential panic in MPT path removal | `Node::remove` with a path shorter than the extension's compressed path | [#465](https://github.com/midnightntwrk/midnight-ledger/pull/465) |
| Fix: potential panic in bridge fee processing | Removes an `assert`-driven panic in fee processing | [#467](https://github.com/midnightntwrk/midnight-ledger/pull/467) |
| Fix: non-associativity of Dust event processing | Deterministic Dust event replay | regression test [#556](https://github.com/midnightntwrk/midnight-ledger/pull/556) |
| Fix: tighten cost heuristic — fewer transactions pushed to the fallible section | Less over-conservative cost estimation | see changelog |

### No migration from ledger 8

There is no state-transition / on-chain migration from ledger 8 to ledger 9. The ledger storage layout is incompatible. Fresh-chain start is required; existing QA/preview/preprod chains must be re-genesis'd.

Mixed-chain support (replaying pre-v9 blocks into a v9 node, toolkit caching across version boundaries) is "very relaxed" — meaning it is present in the code structure but explicitly not tested or supported in this release. Tests requiring `intent[v7]` or a hard-fork from ledger v8→v9 are marked `#[ignore]`.

### Removed function

`construct_distribute_treasury_system_tx` (a ledger-v8 host API function) has been removed. It was called only from the c2m-bridge pallet and was invoked incorrectly there. Because the bridge was never enabled, the function was never actually executed; removal is safe.

---

## Cardano→Midnight bridge

### Architecture

The c2m-bridge pallet lives in `pallets/c2m-bridge/` and implements `pallet_partner_chains_bridge::TransferHandler<BridgeRecipient>`. The partner-chains bridge pallet handles Cardano observation and inherent data delivery; the c2m-bridge pallet contains all Midnight-specific dispatch logic.

The inherent data provider (`TokenBridgeInherentDataProvider`) is version-aware and reports `Inert` unless the runtime pallet is present **and** `MainChainScripts` and a data checkpoint have been configured via governance. In this alpha neither has been set, so the IDP stays `Inert` everywhere.

### Transfer classification

Transfers arrive from the partner-chains bridge as `BridgeTransferV1<BridgeRecipient>` with a `TransferRecipient` variant:

| Variant | Source on Cardano | Midnight action |
| ------- | ----------------- | --------------- |
| `Address { .. }` | ICS validator input | Credit recipient (if pre-approved) |
| `Reserve` | Reserve validator input | `construct_distribute_reserve_system_tx` |
| `Invalid` | Malformed metadata | `construct_unlock_to_treasury_system_tx` |

Classification is based on **which validator's UTxO is consumed** (ICS vs Reserve), not on transaction metadata. Earlier code used metadata for this, which was attackable on the midnight reserve pool.

### Pre-approvals filter

User (`Address`) transfers are subject to a single-use governance pre-approval check:

- The governance origin calls `add_approved_mc_tx_hashes` to whitelist one or more Cardano tx hashes before they arrive on-chain.
- When a user transfer arrives, the approval entry is `take`n (atomic single-use removal) before the ledger system tx is constructed. A failed ledger call therefore cannot be replayed against the same approval.
- If no approval entry exists, the funds are redirected to the Treasury and an `UnapprovedTransfer` event is emitted.

### Subminimal transfer accumulation

Transfers whose STAR amount is below the ledger's `c_to_m_bridge_min_amount` parameter are accumulated in `SubminimalTransfers` storage. When the running sum exceeds `SubminimalTransfersConfig::subminimal_transfers_flush_threshold`, the entire accumulated sum is flushed as a single `construct_unlock_to_treasury_system_tx` and a `SubminimalFlushTransfer` event is emitted. The genesis `SubminimalTransfersConfig` is set in the chain spec.

### Denomination: STAR not NIGHT

All amounts flowing through the bridge are in **STAR** (the Midnight ledger's base unit). Cardano records amounts in STAR; the ledger operates in STAR. A prior version of the pallet incorrectly applied a NIGHT→STAR denomination conversion; that conversion has been removed (PR #1608).

### Pallet surface

#### Extrinsics (call index)

| Index | Name | Origin | Purpose |
| ----- | ---- | ------ | ------- |
| 0 | `set_subminimal_transfers_config(config: SubminimalTransfersConfig)` | `GovernanceOrigin` | Update the subminimal flush threshold |
| 1 | `add_approved_mc_tx_hashes(hashes: BoundedVec<McTxHash, 32>)` | `GovernanceOrigin` | Whitelist up to 32 Cardano tx hashes for user transfers |

#### Events

| Event | Fields | Emitted when |
| ----- | ------ | ------------ |
| `UserTransfer` | `mc_tx_hash`, `amount`, `recipient`, `midnight_tx_hash` | Approved user transfer executed |
| `ReserveTransfer` | `mc_tx_hash`, `amount`, `midnight_tx_hash` | Reserve transfer executed |
| `InvalidTransfer` | `mc_tx_hash`, `amount`, `midnight_tx_hash` | Invalid transfer redirected to treasury |
| `UnapprovedTransfer` | `mc_tx_hash`, `amount`, `recipient`, `midnight_tx_hash` | User transfer without pre-approval redirected to treasury |
| `SubminimalFlushTransfer` | `amount`, `count`, `midnight_tx_hash` | Accumulated subminimal transfers flushed to treasury |

#### Storage items

| Item | Type | Purpose |
| ---- | ---- | ------- |
| `SubminimalTransfersConfiguration` | `SubminimalTransfersConfig` (ValueQuery) | Current flush threshold config |
| `SubminimalTransfers` | `SubminimalTransfersState { count: u32, sum: u64 }` (ValueQuery) | Running accumulator |
| `TransferCounter` | `u32` (ValueQuery) | Per-block nonce counter; killed on `on_finalize` |
| `ApprovedMcTxHashes` | `Map<Blake2_128Concat, McTxHash, ()>` | Governance-approved tx hash set |

#### Runtime API

Declared in `pallets/c2m-bridge/src/runtime_api.rs`:

```rust
pub trait C2MBridgeApi {
    fn get_approved_mc_tx_hashes() -> Vec<McTxHash>;
}
```

### Toolkit: bridge-transfer command

`midnight-toolkit bridge-transfer` submits a Cardano transaction from a user wallet to the ICS validator address with bridge metadata. It requires Ogmios to be reachable.

```text
midnight-toolkit bridge-transfer \
  --signing-key <PATH>         # Cardano payment signing key JSON file
  --ics-config <PATH>          # ICS configuration JSON (ICS address + cNight asset id)
  --amount <u64>               # cNight tokens to transfer (in STAR)
  [--recipient-address <HEX>]  # 32-byte hex Midnight UserAddress (conflicts with --invalid)
  [--invalid]                  # Produce malformed metadata (ends in Treasury; for testing)
  [-O, --ogmios-url <URL>]     # default ws://localhost:1337; env: OGMIOS_URL
```

Exactly one of `--recipient-address` or `--invalid` must be supplied; they conflict with each other and omitting both is an error.

The transaction carries metadata key `6500973`. The metadata value is either a single-element CBOR list containing the 32-byte recipient address (user transfer), or the text string `"this is invalid bridge tx metadata"` (invalid transfer).

Reserve transfers are not directly initiatable via this command; they are handled by the partner-chains reserve management flow on the Cardano side.

### SessionInfoApi

Crate: `midnight-primitives-session-info` (`primitives/session-info/`).

```rust
pub trait SessionInfoApi {
    fn current_session_index() -> u32;
}
```

Backed in the runtime by `pallet_partner_chains_session::Pallet::current_index()`.

---

## References

- [PR #1604](https://github.com/midnightntwrk/midnight-node/pull/1604) — Ledger 9 support
- [PR #1386](https://github.com/midnightntwrk/midnight-node/pull/1386) — Added c2m-bridge pallet
- [PR #1333](https://github.com/midnightntwrk/midnight-node/pull/1333) — Bridge chain-spec / genesis configuration
- [PR #1513](https://github.com/midnightntwrk/midnight-node/pull/1513) — Fixes Reserve Transfer classification
- [PR #1477](https://github.com/midnightntwrk/midnight-node/pull/1477) — Pre-approvals filter
- [PR #1393](https://github.com/midnightntwrk/midnight-node/pull/1393) — Subminimal transfer accumulation
- [PR #1608](https://github.com/midnightntwrk/midnight-node/pull/1608) — Fix unnecessary denomination (STAR not NIGHT)
- [PR #1340](https://github.com/midnightntwrk/midnight-node/pull/1340) — bridge-transfer toolkit command
- [PR #1534](https://github.com/midnightntwrk/midnight-node/pull/1534) — SessionInfoApi
- [Release notes](release-notes-2.0.0-alpha.1.md)
