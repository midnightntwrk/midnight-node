<!-- markdownlint-disable MD012 MD013 MD014 MD022 MD031 MD032 MD033 MD034 MD060 -->

# Runtime Diff: 1.0.300 → 2.1.0-rc.2

This is the **first measured** runtime diff on the 2.1.0 line. `node-2.1.0-rc.2` is the first release
of the line to publish a deterministic srtool-built WASM asset, so the current side is a published
blob rather than a reconstruction. The equivalent document for `2.1.0-beta.1`
(`docs/release-notes/2.1.0-beta.1/runtime-diff.md`, added by PR
[#2063](https://github.com/midnightntwrk/midnight-node/pull/2063)) had to infer its prior side by
reading source; where the two disagree, this one is authoritative.

**What the two sides are.**

- **Prior side — `1.0.300`**: the runtime devnet actually ran before the 2.1.0 hard fork, which
  makes it the real fork-from baseline. There is no GitHub release for it and no published WASM
  asset, so the blob is extracted from the published `1.0.300` node image at
  `/artifacts-amd64/midnight_node_runtime.compact.compressed.wasm`
  (`docker create` + `docker cp`). Identify it by
  `sha256:985a361e9b74a7037fbebe3831c9c3f0504be010a1ba957b301ce25326011546`.

- **Current side — `2.1.0-rc.2`**: `midnight_node_runtime-2.1.0-rc.2.compact.compressed.wasm`, the
  srtool-built asset published on
  [`node-2.1.0-rc.2`](https://github.com/midnightntwrk/midnight-node/releases/tag/node-2.1.0-rc.2).

**Note on the earlier documented baseline.** `2.1.0-beta.1` and `2.1.0-rc.1` name `1.0.3` as the
fork-from baseline; that release is still pending. `1.0.300` is what is actually deployed, and it is
what PR [#2161](https://github.com/midnightntwrk/midnight-node/pull/2161) (toolkit accepts spec
`001_000_300`) and PR [#2175](https://github.com/midnightntwrk/midnight-node/pull/2175) (devnet
chain-spec copied from 1.0.300) are written against. The earlier documents' pallet sections are not
wrong: the metadata surface of the `1.0.300` blob is **byte-identical** to that of the published
`1.0.0` blob those documents were built from. Running the same `subwasm diff` against both prior
sides produces output that differs on exactly one line — the SCALE-encoded `Version` constant, where
`spec_version` reads `1_000_000` versus `1_000_300`.

## Runtime metadata

| Field | `1.0.300` (prior) | `2.1.0-rc.2` (current) |
| --- | --- | --- |
| `spec_name` | `midnight` | `midnight` |
| `spec_version` | `001_000_300` | `002_001_000` |
| `impl_version` | `0` | `0` |
| `transaction_version` | `3` | `4` |
| `authoring_version` | `1` | `1` |
| Metadata version | V14 | V14 |
| Registered runtime APIs | 24 | 26 |
| Blake2-256 of the blob | `0xcc2b4b1720f92170b47e13e09dc8512a5da499becee5105336a5eef4e972729d` | `0xb15f7383617c41cdb1b74b1ea4c8db0f4d0cde5e8438708a89e9f1df343a01b3` |
| SHA-256 of the blob | `0x985a361e9b74a7037fbebe3831c9c3f0504be010a1ba957b301ce25326011546` | `0xcb1df875b0adc841893860ca64f2f0fa441dda06703834deec2dcdb2815400cf` |
| `system.setCode` hash | `0xda8a58491061d04462e0282d6f9caa6a3eb4a4fb68e5618b299eceb45e27a9c3` | `0x661ab01fc058cdd0bccb33b0372868eae3e6f8ef3c4263a7a63f93e6d2c5fed3` |
| `authorizeUpgrade` hash | `0x0d658bbd941164d10f2acd1bda99c431b47009d935c9a2baaec73df5a3006f7a` | `0x1b7e536898fe8ef38d698374849745729b661fbd743458a06d1f0e39edc90c0b` |
| Compressed size | 544,220 bytes (81.41%) | 556,781 bytes (84.28%) |
| Built by | — (extracted from image) | srtool `v0.18.5`, `rustc 1.98.1 (48a229cea 2026-09-01)` (`srtool-digest.json`) |

## Summary

`spec_version` moves `001_000_300` → `002_001_000` and `transaction_version` moves `3` → `4`. Two
pallets are added — `SafeMode` (index 20) and `C2MBridge` (index 33) — and no pallet is removed. Two
runtime APIs are added and none removed. `CNightObservation` carries by far the largest change: two
new root calls, five new events, four new storage items (one renamed), and a wholesale renumbering
of its error variants.

> **`transaction_version` bumped `3` → `4`. This does not break SCALE decoding.**
> `frame_system::CheckTxVersion` (`runtime/src/lib.rs`) mixes the version into a signed payload's
> *implicit* data, not its encoded bytes. Previously-signed extrinsics still decode fine; they fail
> **signature verification**. The required action for wallet and SDK authors is to **refresh runtime
> metadata and re-sign** — no codec change is needed. Anything holding a pre-signed, un-submitted
> extrinsic against the old runtime must rebuild it.

Note that `subwasm`'s own heuristic reports `Require transaction_version bump: false` in the summary
below. That is not a contradiction: its reduced differ only inspects call signatures, and no call
signature changed incompatibly. The bump is a deliberate, conservative one made earlier on this line
for the ledger 8 → 9 work, and it is already in force.

## Pallet changes

### Added

| Index | Pallet | Why |
| --- | --- | --- |
| 20 | `SafeMode` | Introduced in `2.1.0-rc.1` as the failed-multi-block-migration handler: the chain enters safe mode instead of freezing permanently (PR [#2079](https://github.com/midnightntwrk/midnight-node/pull/2079)). |
| 33 | `C2MBridge` | The Cardano-to-Midnight bridge pallet (PR [#1386](https://github.com/midnightntwrk/midnight-node/pull/1386)). Inert until a governance action sets its `MainChainScripts` and data checkpoint. |

### Removed

None.

### Modified

#### `System` (index 0)

- **Events** — `CodeUpdated` gains a `hash: T::Hash` argument.
- **Constants** — `Version` changes, as expected from the `spec_version` / `transaction_version`
  move. The byte-level diff is elided in the raw output below; the decoded values are in the
  metadata table above.

#### `Midnight` (index 5)

- **Errors** — two added: `ContractNotPresent` (13) and `BeneficiaryNotFound` (14). These let
  callers distinguish a missing contract from an empty one, and an absent beneficiary from a
  zero balance (PRs [#916](https://github.com/midnightntwrk/midnight-node/pull/916),
  [#1359](https://github.com/midnightntwrk/midnight-node/pull/1359)).

#### `MidnightSystem` (index 6)

- **Errors** — the single catch-all `LedgerApiError` is **removed** and replaced by fifteen granular
  variants at indices 2–16: `Deserialization`, `Serialization`, `Transaction`, `LedgerCacheError`,
  `NoLedgerState`, `LedgerStateScaleDecodingError`, `ContractCallCostError`,
  `BlockLimitExceededError`, `FeeCalculationError`, `HostApiError`, `GetTransactionContextError`,
  `ContractNotPresent`, `BeneficiaryNotFound`, `SystemTransactionNotAllowedForCNight`,
  `SystemTransactionNotAllowedForBridge` (PR
  [#1449](https://github.com/midnightntwrk/midnight-node/pull/1449)).
- **Client impact**: any consumer that matches on this pallet's error *index* must be updated.
  Index 2 was `LedgerApiError`; it is now `Deserialization`.

#### `CNightObservation` (index 13)

- **Calls** — two root extrinsics added (PR
  [#1602](https://github.com/midnightntwrk/midnight-node/pull/1602)):
  - `set_cnight_identifier(policy_id: [u8; CNIGHT_POLICY_ID_LENGTH], asset_name: BoundedVec<u8, ConstU32<CARDANO_ASSET_NAME_MAX_LENGTH>>)` at index 3
  - `set_auth_token_asset_name(asset_name: BoundedVec<u8, ConstU32<CARDANO_ASSET_NAME_MAX_LENGTH>>)` at index 4
- **Events** — five added, all covering the post-hardfork DUST re-apply migration (PR
  [#2012](https://github.com/midnightntwrk/midnight-node/pull/2012)): `DustReapplyStarted` (5),
  `DustReapplyBatchFailed { nonces }` (6), `DustReapplyCompleted { applied, skipped }` (7),
  `DustReapplySkipped { applied, skipped }` (8), `ObservationsSkippedForMigration` (9).
- **Storage** — added `DustReapplyCtime`, `DustReapplyProgress`, `PreForkStateKey`; `Mappings` is
  **renamed to** `Mapping`.
- **Errors** — every index from 1 upward shifts. This is the sharpest client-compatibility edge in
  the whole diff:

  | Index | `1.0.300` | `2.1.0-rc.2` |
  | --- | --- | --- |
  | 1 | `MaxRegistrationsExceeded` | `NonAsciiAssetName` |
  | 2 | `LedgerApiError` | `InherentAlreadyExecuted` |
  | 3 | `InherentAlreadyExecuted` | `CardanoPositionRegression` |
  | 4 | `CardanoPositionRegression` | `TooManyUtxos` |
  | 5 | `TooManyUtxos` | `Deserialization` |
  | 6–17 | — | `Serialization`, `Transaction`, `LedgerCacheError`, `NoLedgerState`, `LedgerStateScaleDecodingError`, `ContractCallCostError`, `BlockLimitExceededError`, `FeeCalculationError`, `HostApiError`, `GetTransactionContextError`, `ContractNotPresent`, `BeneficiaryNotFound` |

#### `Bridge` (index 32)

- **Events** — `Transfer` removed.

#### `FederatedAuthority` (index 44)

- **Errors** — three unused variants removed: `MotionTooEarlyToClose`, `MotionAlreadyExists`,
  `MotionExpired` (PR [#938](https://github.com/midnightntwrk/midnight-node/pull/938)).

## Runtime APIs

Metadata V14 does not carry runtime API definitions, so `subwasm diff` cannot report them. The delta
below is read from the `apis` registry inside each blob's `RuntimeVersion` (`subwasm info --json`),
which is authoritative for what the deployed runtime advertises. Each id is `blake2_64` of the trait
name; all 24 prior-side ids resolve against the traits in `runtime/src/lib.rs`.

### Added runtime APIs

| API id | Trait | Version |
| --- | --- | --- |
| `0x817793cea3bd352e` | `pallet_c2m_bridge::C2MBridgeApi` | 1 |
| `0x85570eddd92bc2f7` | `midnight_primitives_session_info::SessionInfoApi` | 1 |

`SessionInfoApi` exposes the Substrate session index (PR
[#1534](https://github.com/midnightntwrk/midnight-node/pull/1534)). `C2MBridgeApi` accompanies the
new `C2MBridge` pallet.

### Removed or modified runtime APIs

None. All 24 APIs present in `1.0.300` are present in `2.1.0-rc.2` at the **same version number** —
no API had its version bumped, so no existing runtime API call signature changed.

## What in this diff is new for `rc.2`

**Nothing.** Every entry in this document was already true of `2.1.0-rc.1`. The whole metadata
surface change belongs to the `1.0.300 → 2.1.0-beta.1 → 2.1.0-rc.1` steps; `rc.2` adds no pallet,
call, event, error, storage item, constant or runtime API, and renames none.

This is measured, not inferred. `rc.1` published no WASM asset, but its node image carries one:

```shell
CID=$(docker create ghcr.io/midnightntwrk/midnight-node:2.1.0-rc.1)
docker cp "$CID:/artifacts-amd64/midnight_node_runtime.compact.compressed.wasm" rc1.wasm
docker rm -f "$CID"
subwasm diff rc1.wasm midnight_node_runtime-2.1.0-rc.2.compact.compressed.wasm
```

```text
No change detected
SUMMARY:
- Compatible.......................: true
- Require transaction_version bump.: false
```

The runtime API registry agrees: both blobs advertise the same 26 APIs at the same version numbers,
none added, none removed.

| Field | `2.1.0-rc.1` | `2.1.0-rc.2` |
| --- | --- | --- |
| `spec_version` | `002_001_000` | `002_001_000` |
| `transaction_version` | `4` | `4` |
| Registered runtime APIs | 26 | 26 (identical set and versions) |
| `subwasm diff` vs the other | — | `No change detected` |
| Blake2-256 of the blob | `0x848325bd5a34d81a47215f8e2c27aadad2e9bc6a07b22130f792762baf91da1d` | `0xb15f7383617c41cdb1b74b1ea4c8db0f4d0cde5e8438708a89e9f1df343a01b3` |
| SHA-256 of the blob | `0x34239ebaa232d4cfbe377279ad78b5b987edf0666d75684975ba0e7d74755055` | `0xcb1df875b0adc841893860ca64f2f0fa441dda06703834deec2dcdb2815400cf` |
| Compressed size | 570,228 bytes | 556,781 bytes |
| Built by | Earthly, in-image | srtool `v0.18.5` (published asset) |

### So what *did* change in the blob

The two blobs are not byte-identical — different hash, 13,447 bytes smaller — while their metadata
is. Three things account for that, none of them visible to a metadata consumer:

- **Re-measured weights** (PR [#2160](https://github.com/midnightntwrk/midnight-node/pull/2160)).
  V14 metadata encodes types, calls, events, errors, storage and constants — **not weights**. A
  weight change is real and consensus-relevant for block building, and completely invisible to
  `subwasm diff`. This is the substantive runtime change in `rc.2`; see the release notes.
- **A new compiler** — `rustc 1.98.1` rather than `1.95` (PR
  [#2166](https://github.com/midnightntwrk/midnight-node/pull/2166)). Different codegen, identical
  type-derived metadata.
- **A different builder** — `rc.1`'s blob is the Earthly build extracted from its image, `rc.2`'s is
  the deterministic srtool asset. The size gap is largely this, so do not read it as a source
  change.

The practical consequence for clients: **a consumer that resolved metadata against an `rc.1` chain
does not need to re-fetch or restart before pointing at an `rc.2` one.** The restart guidance in the
`2.1.0-rc.1` notes applies only to the `beta.1` → `rc.x` step.

The practical consequence for operators: three distinct blobs now advertise `spec_version`
`002_001_000`, so a `set_code` proposal must be identified by blob hash, not by version.

## Raw subwasm diff

Generated with `subwasm v0.21.3-1f3a6f67587`. One line — the SCALE-encoded `Version` constant's
byte-level diff — is truncated; its decoded values are in the metadata table above.

```text
[≠] pallet 0: System -> 2 change(s)
  - events changes:
    [≠]  2: CodeUpdated ( )  )
        [Signature(SignatureChange { args: [Added(0, ArgDesc { name: "hash", ty: "T::Hash" })] })]

  - constants changes:
    [≠] Version: [ 32, 109, 105, 100, 110, 105, 103, 104, 116, 32, 109, 105, 100, 110, 105, 103, 104, 116, 1, 0, 0, 0, 108, 67, 15, 0, 0, 0, 0, 0, 96, 251, ... ]
        [Value([Changed(22, U8Change(108, 104)), Changed(23, U8Change(67, 136)), Changed(24, U8Change(15, 30)), Changed(30, U8Change(96, 104)), Changed(79, U8Change(55, 129)), Changed(80, U8Change(227, 119)), Changed(81, U8Change(151, 147)), Changed(82, U8Change(252, 206)), Changed(83, U8Change(124, …  [truncated: SCALE-encoded RuntimeVersion byte diff — the decoded values are in the header table above] ]

[≠] pallet 5: Midnight -> 2 change(s)
  - errors changes:
    [+] ErrorDesc { index: 13, name: "ContractNotPresent" }
    [+] ErrorDesc { index: 14, name: "BeneficiaryNotFound" }

[≠] pallet 6: MidnightSystem -> 16 change(s)
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
    [+] ErrorDesc { index: 15, name: "SystemTransactionNotAllowedForCNight" }
    [+] ErrorDesc { index: 16, name: "SystemTransactionNotAllowedForBridge" }
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

[+] id: 20 - new pallet: SafeMode
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

```
