<!-- markdownlint-disable MD012 MD013 MD014 MD022 MD031 MD032 MD033 MD034 MD060 -->

# Runtime Diff: 1.0.3 → 2.1.0-beta.1

`1.0.3` is the supported fork-from baseline for 2.1.0. That release is still **pending**, so no
1.0.3 runtime WASM exists to feed `subwasm` directly, and this pre-release publishes no WASM asset
of its own either — see [#2061](https://github.com/midnightntwrk/midnight-node/issues/2061).

**How the prior side was obtained.** Relative to `node-1.0.1`, the `release/node-1.0.3` branch
changes exactly two things inside `runtime/`: `spec_version` (`001_000_000` → `001_000_003`) and
`system_version` (`1` → `3`). The only other file it touches under `runtime/` or `pallets/` is
`pallets/midnight/src/tests.rs` (plus one dev-dependency), which does not reach the runtime blob:

```shell
git diff --stat node-1.0.1 origin/release/node-1.0.3 -- runtime/ pallets/
```

So **1.0.3's pallet metadata surface is byte-for-byte 1.0.1's**, and every pallet, storage, call,
event, error and runtime-API change below — everything except the `System::Version` constant — holds
unchanged for 1.0.3 → 2.1.0-beta.1. The pallet-surface diff is therefore taken between the published
`1.0.1` blob and `2.1.0-beta.1` (both extracted from their node images at `/artifacts-amd64/`,
`subwasm` v0.21.3), while the version fields in the table below are read from
`release/node-1.0.3`'s source.

## Metadata

| Field | 1.0.3 (`release/node-1.0.3`) | 2.1.0-beta.1 |
| --- | --- | --- |
| `spec_name` | `midnight` | `midnight` |
| `spec_version` | 1_000_003 | **2_001_000** |
| `impl_version` | 0 | 0 |
| `transaction_version` | 3 | **4** |
| `authoring_version` | 1 | 1 |
| `system_version` | 3 | 3 |
| Metadata version | V14 | V14 |
| Blake2-256 | not published (release pending) | `0x61ee82de20ecbebfa682cc7b8f7e7cec0e00cdeb6d2879a8ce6f7302b1947f71` |
| Compressed size | not published (release pending) | 565,196 bytes (81.92%) |
| `system.setCode` hash | not published (release pending) | `0xd2224138194fa98b36d153ea7807c4c3d3d8c66f3a0c377c8ca9493b36938729` |
| `authorizeUpgrade` hash | not published (release pending) | `0x92604f4efabd79e682db114d9f99c3ab5a1eb5e2b2bcbc39cfaf6367ba383ba7` |

For reference, the 1.0.1 blob the pallet diff was taken against has Blake2-256
`0x11504f7312a72b7d4b4ec26dc5c31788d4d704e3f4a9f026fd77b5823ef9e027`, is 543,996 bytes compressed
(81.41%), and reports core version `midnight-1000000 (midnight-0.tx3.au1)`. 2.1.0-beta.1 reports
`midnight-2001000 (midnight-0.tx4.au1)`.

## Summary

`spec_version` bumps 1_000_003 → 2_001_000 and **`transaction_version` bumps 3 → 4**. A
`transaction_version` bump means signed extrinsics encoded against the 1.0.x runtime will not decode
under 2.1.0: every client, SDK, and indexer must re-fetch metadata at or after the `set_code` block,
and any pre-signed or queued extrinsic must be rebuilt.

**`system_version` does not change — 1.0.3 already ships 3.** From `system_version: 2` onward
`frame_system` stages new code in `:pending_code` and applies it at the start of the *next* block,
rather than overwriting `:code` inside the `set_code` block itself. That means the block applying
this runtime is not a skew block: `:code` and the ledger `StateKey` it is read against stay in step
across the boundary, so the pathology in
[#1959](https://github.com/midnightntwrk/midnight-node/issues/1959) cannot arise when forking from
1.0.3.

One pallet is added (`C2MBridge`, index 33), none removed. Four existing pallets change their
error/event/storage surface, and `CNightObservation` renumbers its error variants.

`subwasm`'s reduced differ reports `Compatible: true` / `Require transaction_version bump: false`.
That verdict is about call indices and call signatures, which indeed did not change
incompatibly; it does not account for the `transaction_version` bump that was made deliberately,
nor for storage or error-index changes. Treat the bump above as authoritative.

## Pallet changes

**Added:**

| Pallet | Index | Notes |
| --- | --- | --- |
| `C2MBridge` | 33 | Cardano-to-Midnight bridge pallet. Ships inert: without `MainChainScripts` configured its inherent data provider reports the `Inert` variant, so a governance action is required to enable it. |

**Removed** — none.

**Modified:**

### `System` (index 0)

- Event `CodeUpdated` gained an argument: `hash: T::Hash`.
- Constant `Version` changed (the SCALE-encoded `RuntimeVersion` — `spec_version`,
  `transaction_version`, `system_version` and the runtime API list, per the header table).

### `Midnight` (index 5)

- Errors added: `ContractNotPresent` (13), `BeneficiaryNotFound` (14).

### `MidnightSystem` (index 6)

- Errors added: `Deserialization` (2), `Serialization` (3), `Transaction` (4), `LedgerCacheError`
  (5), `NoLedgerState` (6), `LedgerStateScaleDecodingError` (7), `ContractCallCostError` (8),
  `BlockLimitExceededError` (9), `FeeCalculationError` (10), `HostApiError` (11),
  `GetTransactionContextError` (12), `ContractNotPresent` (13), `BeneficiaryNotFound` (14).
- Error removed: `LedgerApiError` — the single catch-all is replaced by the granular variants
  above ([#1449](https://github.com/midnightntwrk/midnight-node/pull/1449)).

### `CNightObservation` (index 13)

- Calls added: `set_cnight_identifier(policy_id, asset_name)` (3),
  `set_auth_token_asset_name(asset_name)` (4) — both root-only
  ([#1602](https://github.com/midnightntwrk/midnight-node/pull/1602)).
- Events added: `DustReapplyStarted` (5), `DustReapplyBatchFailed { nonces }` (6),
  `DustReapplyCompleted { applied, skipped }` (7), `DustReapplySkipped { applied, skipped }` (8),
  `ObservationsSkippedForMigration` (9) — the ledger 8→9 dust replay
  ([#2012](https://github.com/midnightntwrk/midnight-node/pull/2012)).
- Storage added: `DustReapplyCtime` (Optional), `DustReapplyProgress` (Default),
  `PreForkStateKey` (Optional), `Mapping` (Optional).
- Storage removed: `Mappings` — renamed to `Mapping`.
- **Errors renumbered.** `MaxRegistrationsExceeded` is gone and every remaining variant shifted
  down one index, then twelve new variants were appended:

  | Index | 1.0.1 | 2.1.0-beta.1 |
  | --- | --- | --- |
  | 1 | `MaxRegistrationsExceeded` | `NonAsciiAssetName` |
  | 2 | `LedgerApiError` | `InherentAlreadyExecuted` |
  | 3 | `InherentAlreadyExecuted` | `CardanoPositionRegression` |
  | 4 | `CardanoPositionRegression` | `TooManyUtxos` |
  | 5 | `TooManyUtxos` | `Deserialization` |
  | 6–17 | — | `Serialization`, `Transaction`, `LedgerCacheError`, `NoLedgerState`, `LedgerStateScaleDecodingError`, `ContractCallCostError`, `BlockLimitExceededError`, `FeeCalculationError`, `HostApiError`, `GetTransactionContextError`, `ContractNotPresent`, `BeneficiaryNotFound` |

  Tooling that maps `DispatchError::Module { index, error }` to a name from a hardcoded table,
  rather than from metadata fetched at the block being decoded, will mis-report these.

### `Bridge` (index 32)

- Event removed: `Transfer`.

### `FederatedAuthority` (index 44)

- Errors removed: `MotionTooEarlyToClose`, `MotionAlreadyExists`, `MotionExpired`
  ([#938](https://github.com/midnightntwrk/midnight-node/pull/938) reworked motion lifecycle
  handling).

## Runtime APIs

V14 metadata does not carry runtime API definitions, so `subwasm` cannot diff them. Taken from
`runtime/src/lib.rs` at each tag instead:

**Added:**

- `pallet_c2m_bridge::C2MBridgeApi<Block>`
- `midnight_primitives_session_info::SessionInfoApi<Block>` — exposes the substrate session index
  ([#1534](https://github.com/midnightntwrk/midnight-node/pull/1534))

**Removed** — none.

**Modified** — none. `MidnightRuntimeApi` is still `#[api_version(5)]` with an unchanged method
set. (The `get_transaction_cost` "version 2" referenced by the dust-replay change file is the
*host function* version, not the runtime API version.)

## Raw subwasm diff

Taken between the published 1.0.1 and 2.1.0-beta.1 blobs, per the note at the top of this
document. The `Version` constant's byte-level diff is elided — it is the SCALE encoding of the
`RuntimeVersion` struct, and on this side of the comparison it carries 1.0.1's `spec_version` and
`system_version` rather than 1.0.3's; use the header table for the real version delta. Everything
else is verbatim, and holds identically for 1.0.3 → 2.1.0-beta.1.

```text
!!! THE SUBWASM REDUCED DIFFER IS EXPERIMENTAL, DOUBLE CHECK THE RESULTS !!!
[≠] pallet 0: System -> 2 change(s)
  - events changes:
    [≠]  2: CodeUpdated ( )  )
        [Signature(SignatureChange { args: [Added(0, ArgDesc { name: "hash", ty: "T::Hash" })] })]

  - constants changes:
    [≠] Version: [ 32, 109, 105, 100, 110, 105, 103, 104, 116, 32, 109, 105, 100, 110, 105, 103, 104, 116, 1, 0, 0, 0, 64, 66, 15, 0, 0, 0, 0, 0, 96, 251, ... ]
        [Value([...184 U8Change(..), 24 Added(..) — SCALE bytes of the RuntimeVersion constant; see the header table above...])]

[≠] pallet 5: Midnight -> 2 change(s)
  - errors changes:
    [+] ErrorDesc { index: 13, name: "ContractNotPresent" }
    [+] ErrorDesc { index: 14, name: "BeneficiaryNotFound" }

[≠] pallet 6: MidnightSystem -> 14 change(s)
  - errors changes:
    [+] ErrorDesc { index: 2, name: "Deserialization" }
    [+] ErrorDesc { index: 3, name: "Serialization" }
    [+] ErrorDesc { index: 4, name: "Transaction" }
    [+] ErrorDesc { index: 5, name: "LedgerCacheError" }
    [+] ErrorDesc { index: 6, name: "NoLedgerState" }
    [+] ErrorDesc { index: 7, name: "LedgerStateScaleDecodingError" }
    [+] ErrorDesc { index: 8, name: "ContractCallCostError" }
    [+] ErrorDesc { index: 9, name: "BlockLimitExceededError" }
    [+] ErrorDesc { index: 10, name: "FeeCalculationError" }
    [+] ErrorDesc { index: 11, name: "HostApiError" }
    [+] ErrorDesc { index: 12, name: "GetTransactionContextError" }
    [+] ErrorDesc { index: 13, name: "ContractNotPresent" }
    [+] ErrorDesc { index: 14, name: "BeneficiaryNotFound" }
    [-] "LedgerApiError"

[≠] pallet 13: CNightObservation -> 29 change(s)
  - calls changes:
    [+] CallDesc { index: 3, name: "set_cnight_identifier", signature: SignatureDesc { args: [ArgDesc { name: "policy_id", ty: "[u8; CNIGHT_POLICY_ID_LENGTH as usize]" }, ArgDesc { name: "asset_name", ty: "BoundedVec<u8, ConstU32<CARDANO_ASSET_NAME_MAX_LENGTH>>" }] } }
    [+] CallDesc { index: 4, name: "set_auth_token_asset_name", signature: SignatureDesc { args: [ArgDesc { name: "asset_name", ty: "BoundedVec<u8, ConstU32<CARDANO_ASSET_NAME_MAX_LENGTH>>" }] } }

  - events changes:
    [+] EventDesc { index: 5, name: "DustReapplyStarted", signature: SignatureDesc { args: [] } }
    [+] EventDesc { index: 6, name: "DustReapplyBatchFailed", signature: SignatureDesc { args: [ArgDesc { name: "nonces", ty: "Vec<T::Hash>" }] } }
    [+] EventDesc { index: 7, name: "DustReapplyCompleted", signature: SignatureDesc { args: [ArgDesc { name: "applied", ty: "u32" }, ArgDesc { name: "skipped", ty: "u32" }] } }
    [+] EventDesc { index: 8, name: "DustReapplySkipped", signature: SignatureDesc { args: [ArgDesc { name: "applied", ty: "u32" }, ArgDesc { name: "skipped", ty: "u32" }] } }
    [+] EventDesc { index: 9, name: "ObservationsSkippedForMigration", signature: SignatureDesc { args: [] } }

  - errors changes:
    [≠]  1: MaxRegistrationsExceeded
        [Name(StringChange("MaxRegistrationsExceeded", "NonAsciiAssetName"))]
    [≠]  2: LedgerApiError  
        [Name(StringChange("LedgerApiError", "InherentAlreadyExecuted"))]
    [≠]  3: InherentAlreadyExecuted
        [Name(StringChange("InherentAlreadyExecuted", "CardanoPositionRegression"))]
    [≠]  4: CardanoPositionRegression
        [Name(StringChange("CardanoPositionRegression", "TooManyUtxos"))]
    [≠]  5: TooManyUtxos    
        [Name(StringChange("TooManyUtxos", "Deserialization"))]
    [+] ErrorDesc { index: 6, name: "Serialization" }
    [+] ErrorDesc { index: 7, name: "Transaction" }
    [+] ErrorDesc { index: 8, name: "LedgerCacheError" }
    [+] ErrorDesc { index: 9, name: "NoLedgerState" }
    [+] ErrorDesc { index: 10, name: "LedgerStateScaleDecodingError" }
    [+] ErrorDesc { index: 11, name: "ContractCallCostError" }
    [+] ErrorDesc { index: 12, name: "BlockLimitExceededError" }
    [+] ErrorDesc { index: 13, name: "FeeCalculationError" }
    [+] ErrorDesc { index: 14, name: "HostApiError" }
    [+] ErrorDesc { index: 15, name: "GetTransactionContextError" }
    [+] ErrorDesc { index: 16, name: "ContractNotPresent" }
    [+] ErrorDesc { index: 17, name: "BeneficiaryNotFound" }

  - storages changes:
    [+] StorageDesc { name: "DustReapplyCtime", modifier: "Optional", default_value: [0] }
    [+] StorageDesc { name: "DustReapplyProgress", modifier: "Default", default_value: [0, 0, 0, 0, 0, 0, 0, 0] }
    [+] StorageDesc { name: "Mapping", modifier: "Optional", default_value: [0] }
    [+] StorageDesc { name: "PreForkStateKey", modifier: "Optional", default_value: [0] }
    [-] "Mappings"

[≠] pallet 32: Bridge -> 1 change(s)
  - events changes:
    [-] "Transfer"

[+] id: 33 - new pallet: C2MBridge
[≠] pallet 44: FederatedAuthority -> 3 change(s)
  - errors changes:
    [-] "MotionTooEarlyToClose"
    [-] "MotionAlreadyExists"
    [-] "MotionExpired"

SUMMARY:
- Compatible.......................: true
- Require transaction_version bump.: false

!!! THE SUBWASM REDUCED DIFFER IS EXPERIMENTAL, DOUBLE CHECK THE RESULTS !!!
```

## Reproducing

```shell
# Confirm 1.0.3 changes nothing in the runtime blob beyond the two version constants.
git diff node-1.0.1 origin/release/node-1.0.3 -- runtime/ pallets/

# Extract the two published blobs and diff them.
for t in 1.0.1 2.1.0-beta.1; do
  cid=$(docker create midnightntwrk/midnight-node:$t)
  docker cp "$cid:/artifacts-amd64/midnight_node_runtime.compact.compressed.wasm" "runtime-$t.wasm"
  docker rm -f "$cid"
done
subwasm info runtime-1.0.1.wasm
subwasm info runtime-2.1.0-beta.1.wasm
subwasm diff runtime-1.0.1.wasm runtime-2.1.0-beta.1.wasm
subwasm diff --json runtime-1.0.1.wasm runtime-2.1.0-beta.1.wasm
```

Once 1.0.3 is released, re-run the diff directly against its blob to confirm the `Version` constant
is the only delta from the run above.

See also the [2.1.0-beta.1 release notes](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/release-notes-2.1.0-beta.1.md) and the
[1.0.3 → 2.1.0 hardfork migration guide](https://github.com/midnightntwrk/midnight-node/blob/main/docs/release-notes/2.1.0-beta.1/migration-hardfork-1.0.x.md).
