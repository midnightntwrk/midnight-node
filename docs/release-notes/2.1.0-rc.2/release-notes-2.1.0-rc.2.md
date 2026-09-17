<!-- markdownlint-disable MD012 MD013 MD014 MD022 MD031 MD032 MD033 MD034 MD060 -->

# Midnight Node 2.1.0-rc.2

## Metadata

- **Type of release**: major — the last final release is `node-1.0.1`; `2.0.0` never shipped a final, so `2.1.0` is the first release of a new major line.
- **Date**: 2026-09-17
- **Ships in bundle**: TBD — standalone pre-release (not bundled). No bundle in `midnightntwrk/midnight-network-ops` references this tag; `releases/components/latest-component-releases.md` still lists `2.1.0-rc.1` as the line's latest pre-release and needs updating.
- **Git tag**: [node-2.1.0-rc.2](https://github.com/midnightntwrk/midnight-node/tree/node-2.1.0-rc.2)
- **Environment**: devnet, qanet. For the full compatibility matrix, see the [release notes overview](https://docs.midnight.network/relnotes/overview).
- **Upgrade scope**: binary + runtime
- **Reset required**: No — with one bootstrap caveat for preview and devnet, see **Deployment information**.
- **Governance action required**: Yes — no *new* action in this delta; the `set_code` enactment carried from `2.1.0-beta.1` still applies.
- **Sister-line note**: This release is on the `2.1.x` line. Operators on the `1.0.x` maintenance line should track `node-1.0.2`. The fork-from baseline is the **`1.0.300`** runtime that devnet actually ran before the hard fork; `2.1.0-beta.1` and `2.1.0-rc.1` named `1.0.3`, which is still pending.

## High-level summary

`2.1.0-rc.2` is a release-engineering candidate: five commits on top of `2.1.0-rc.1`, no new protocol behaviour, no `spec_version` move, and no metadata change. It clears the two things that stood between the line and a publishable final. First, the deterministic srtool runtime WASM is published again — `2.1.0-rc.2` is the **first release of the 2.1.0 line to ship a `*.wasm` asset and an `srtool-digest.json`**, which resolves the headline known issue from `rc.1` and makes a verifiable governance `set_code` artefact available. Second, the 2.1.0 benchmark weights are re-run on the reference machine, correcting a real under-pricing: every call in `pallet_c2m_bridge` and `pallet_partner_chains_bridge` was budgeted at roughly half its cost on validator hardware. Alongside those, two packaged chain-spec corrections fix a defect where a preview or devnet node brought up from an empty disk computed the wrong genesis hash and could not peer.

## Audience

These notes are written for:

- **Full Node Operators (FNOs)** bringing up preview or devnet nodes from an empty disk, or running validators on the `2.1.x` line.
- **Shielded Technologies** engineering, SRE and release engineering, who own the srtool artefact chain and the environment rollout.
- **Midnight Foundation** release coordination, for the governance `set_code` artefact.
- **Developers** using the toolkit to replay devnet history or generate transactions against devnet.
- **External integrators** whose clients, SDKs or indexers decode against the node's metadata — for the one thing they can now stop worrying about, see **Breaking changes**.

## Dependencies

- **Rust 1.98.1** — the toolchain moves from `1.95` across the workspace, the `subxt` and nightly cNIGHT e2e container images, and the srtool image used for deterministic runtime WASM builds (PR [#2166](https://github.com/midnightntwrk/midnight-node/pull/2166), backported as [#2172](https://github.com/midnightntwrk/midnight-node/pull/2172)). No consumer-facing dependency follows from this; it is recorded because it is what made the srtool build work again.
- **midnight-ledger 8.1.2** and **9.1.0.0-rc.5** — unchanged from `2.1.0-rc.1`; the node binary provides both via host calls.
- **The chain must already be running the 1.0.300 runtime** (`spec_version` `001_000_300`) — this is what devnet ran before the 2.1.0 hard fork, and what PRs [#2161](https://github.com/midnightntwrk/midnight-node/pull/2161) and [#2175](https://github.com/midnightntwrk/midnight-node/pull/2175) in this delta are written against. It supersedes the `1.0.3` baseline named in `2.1.0-beta.1` and `2.1.0-rc.1`; the two have an identical pallet metadata surface, so nothing previously documented about the fork surface changes. The measured diff is in [runtime-diff.md](runtime-diff.md).
- Ledger 7 remains fully removed on this line, so this node cannot decode or replay chain history predating the ledger 7 → 8 hardfork.

**Downstream impact (cascading effects).**

- The runtime WASM blob changes (new weights, new compiler) while `spec_version` stays at `002_001_000` — the third distinct blob to claim that version on this line. Unlike the `beta.1 → rc.1` step, **runtime metadata does not change**, so no client, indexer or toolkit needs to re-fetch it. See **Breaking changes** for the evidence and the narrow case that does still bite.
- Packaged network configuration changes for **preview** and **devnet**: `res/preview/` and `res/devnet/` chain-specs and genesis artefacts are replaced. Running nodes are unaffected — their genesis is on disk — but anything that bootstraps from `res/` must use this release's copy. See **Deployment information**.
- The measured bridge weights are roughly 1.92x–2.51x the previous values. Blocks carrying bridge activity will account more weight per call than they did under the `rc.1` runtime, so effective per-block bridge throughput falls to match the real cost. `handle_transfers` is a `Mandatory` inherent, where under-weighting was the unsafe direction.

For all other interop questions, see the bundle dependency matrix.

## Tested-with versions

> These are build pins — not QA-verified — read from the workspace lockfile and pin files at the release tag, so they say what this release was compiled against rather than what it was tested against. Only the Rust toolchain moved relative to `2.1.0-rc.1`.

| Component | Tested-with version |
| --- | --- |
| Rust toolchain | `1.98.1` — **changed** from `1.95` (PR [#2166](https://github.com/midnightntwrk/midnight-node/pull/2166)) |
| srtool | `ghcr.io/shieldedtech/srtool:1.98.1-0.18.5` — **changed** from `paritytech/srtool:1.93.0-0.18.4` (PR [#2166](https://github.com/midnightntwrk/midnight-node/pull/2166)) |
| `midnight-ledger` (ledger 8) | `8.1.2` |
| `midnight-ledger` (ledger 9) | `9.1.0.0-rc.5`, per-crate tags |
| `polkadot-sdk` | `polkadot-stable2606` |
| `partner-chains` | `1.8.1` |
| `compactc` | `0.33.0-rc.1` |
| `subxt` | `0.50.0` and `0.44.3` |
| `parity-db` | `0.5.4` |
| `midnight-indexer` | TODO: not yet available |

## Deployment information

- **Upgrade scope**: binary + runtime — a new binary and image, plus the coordinated on-chain runtime upgrade already carried from `2.1.0-beta.1`. The runtime WASM differs from `rc.1`'s (weights and compiler), though `spec_version` and metadata do not.
- **Reset required**: No for a running node. **Yes for a preview or devnet node that was bootstrapped from `2.1.0-rc.1` or earlier `res/` artefacts** — such a node computed a genesis hash that no live bootnode recognises, was rejected as a different chain, and has been sitting at block #0 with no peers. Wipe its base path and re-bootstrap from this release's `res/`. Nodes already synced on either network are unaffected; their genesis is on disk. Nothing else in this delta calls for a state wipe, re-sync or re-index.
- **Governance action required**: Yes, unchanged — the runtime is swapped by a governance `set_code`, and this is the first release of the line that publishes the deterministic blob and digest that action should reference. Enabling the C2M bridge additionally needs a governance action to set its `MainChainScripts` and data checkpoint; without it the bridge's inherent data provider stays `Inert`.
- **Downtime / coordination**: **`rc.1` → `rc.2` is a rolling binary upgrade.** Nothing in this delta changes consensus-affecting node behaviour: the node-side diff is import fixes from new clippy lints, the runtime diff is weight wiring, and the on-chain runtime is a single blob every validator executes identically. Validators may be swapped one at a time. The `beta.1` ↔ `rc.x` flag-day requirement announced in `2.1.0-rc.1` — driven by the cNIGHT UTXO ordering change in PR [#2123](https://github.com/midnightntwrk/midnight-node/pull/2123) — is unchanged and still applies to anyone still on `beta.1` binaries.

## Artifacts

- `ghcr.io/midnightntwrk/midnight-node:2.1.0-rc.2` — node image; index digest `sha256:55335420d20dddb57e0c03370e9b690546f5c09347b1c2edfc3e7e6e0fedd072` (linux/amd64 `sha256:068a27fcd2c50ca35a0ab3dd3626f42997de7e2ba65e86eb4fa40749722c93ca`, linux/arm64 `sha256:d1afcf24084d3f860a3c66d87e9600383aa200094c04f028eb7c9a7f6703e3c2`)
- `ghcr.io/midnightntwrk/midnight-node-toolkit:2.1.0-rc.2` — toolkit image; index digest `sha256:16a14dd347c1a2476162bbe1949b8e2638506cae9db4189da921347fdc4af9c2` (linux/amd64 `sha256:f24d89f8a5b1367a2f4c0f9788b404ddb558be0f3ebe9f339955131d7077e8b0`, linux/arm64 `sha256:4c0005ed1a25604990229d3bd4c07832f6298a43b42cd5a7b70e1548058d7578`)
- `midnight_node_runtime-2.1.0-rc.2.compact.compressed.wasm` — **deterministic runtime blob, new on this line**; `sha256:cb1df875b0adc841893860ca64f2f0fa441dda06703834deec2dcdb2815400cf`, blake2-256 `0xb15f7383617c41cdb1b74b1ea4c8db0f4d0cde5e8438708a89e9f1df343a01b3`, 556,781 bytes. `system.setCode` proposal hash `0x661ab01fc058cdd0bccb33b0372868eae3e6f8ef3c4263a7a63f93e6d2c5fed3`; `authorizeUpgrade` proposal hash `0x1b7e536898fe8ef38d698374849745729b661fbd743458a06d1f0e39edc90c0b`. **This is the artefact a governance `set_code` should reference.**
- `midnight_node_runtime-2.1.0-rc.2.compact.wasm` — `sha256:0aae87cecacc26f4cc349187204b8cff408207aae9823ef6304b84b8628460a5`
- `midnight_node_runtime-2.1.0-rc.2.wasm` — `sha256:efeb98c35982834218cd56308f88c8f9713959804109198126b0d61699e6c8e9`
- `srtool-digest.json` — build provenance; srtool `v0.18.5`, `rustc 1.98.1 (48a229cea 2026-09-01)`, built `2026-09-17T15:02:40Z`
- `midnight-node-2.1.0-rc.2-linux-amd64.tar.gz` — `sha256:4cf53431f30d7b3caa909daa71b04186b8a670f43819f4fbcee2d2ed8f9e54bd`
- `midnight-node-2.1.0-rc.2-linux-arm64.tar.gz` — `sha256:0e223177ecd8708e75c5ab0f4615e80fc5a286ca6192a66798b56b24ac5f0106`
- `midnight-node-toolkit-2.1.0-rc.2-linux-amd64.tar.gz` — `sha256:54f73db3a69711ca0441ed44683cb6319441aa35fb13fa0784bdb300e28ec6b3`
- `midnight-node-toolkit-2.1.0-rc.2-linux-arm64.tar.gz` — `sha256:f45c17f0b1bc52ed10aa80b90c0917dfd9c72659aa04bd7e0ba69efd7c0e0174`
- `SHA256SUMS` — checksums for all seven binary and WASM assets
- **Git tree hash**: `cacad186a1c0edf9118af1800231255e792e9d4d` (tag `node-2.1.0-rc.2`, commit `4d045ab33`; sibling tags `runtime-2.1.0-rc.2` and `toolkit-2.1.0-rc.2`)

```shell
docker pull ghcr.io/midnightntwrk/midnight-node:2.1.0-rc.2
docker pull ghcr.io/midnightntwrk/midnight-node-toolkit:2.1.0-rc.2
```

## What changed

The list below is the delta against `node-2.1.0-rc.1` — five commits. Everything the `2.x` line introduced relative to `1.0.x` is in the `2.1.0-rc.1`, `2.1.0-beta.1` and `2.0.0-alpha.1` notes and is not restated.

- The deterministic srtool runtime WASM is published again, for the first time on the 2.1.0 line, along with its build digest.
- The whole toolchain moves to Rust 1.98.1 — which is what unblocked the srtool build.
- 2.1.0 benchmark weights are re-run on the reference machine, and four previously unbenchmarked surfaces gain measured weights.
- Bridge pallet weights are corrected upward by 1.92x–2.51x; they had been generated on a developer workstation.
- Preview and devnet ship the chain-spec and genesis artefacts the live networks actually run, fixing a bootstrap-from-empty-disk failure on both.
- The toolkit can replay devnet history again, instead of aborting on pre-fork blocks.

| Change | Upgrade Type | PR |
| --- | --- | --- |
| Deterministic srtool runtime WASM published, with `srtool-digest.json` | Runtime upgrade | [#2166](https://github.com/midnightntwrk/midnight-node/pull/2166) |
| Update Rust toolchain to 1.98.1 across node, toolkit and runtime | Node upgrade + Toolkit + Runtime upgrade | [#2166](https://github.com/midnightntwrk/midnight-node/pull/2166), [#2172](https://github.com/midnightntwrk/midnight-node/pull/2172) |
| 2.1.0 benchmark weights re-run on the reference machine | Runtime upgrade | [#2160](https://github.com/midnightntwrk/midnight-node/pull/2160) |
| Close benchmark coverage gaps: `frame_system_extensions`, `pallet_safe_mode`, `pallet_version`, one cNIGHT setter | Runtime upgrade | [#2160](https://github.com/midnightntwrk/midnight-node/pull/2160) |
| Restore the preview chain-spec and genesis that live preview actually runs | Node upgrade | [#2165](https://github.com/midnightntwrk/midnight-node/pull/2165) |
| Copy the devnet chain-spec and genesis from 1.0.300 after the devnet reset | Node upgrade | [#2175](https://github.com/midnightntwrk/midnight-node/pull/2175) |
| Accept spec version 1.0.300 when replaying chain history | Toolkit | [#2161](https://github.com/midnightntwrk/midnight-node/pull/2161) |
| Runtime metadata diff (subwasm), 1.0.300 → 2.1.0-rc.2 — first measured on this line | Runtime upgrade | [runtime-diff.md](runtime-diff.md) |

## New features

None. This delta ships no new functionality; see **Improvements** and **Fixed defect list**.

## New features requiring configuration updates

None.

## Improvements

### Deterministic runtime WASM is published again

**Description**: `2.1.0-rc.2` is the first release on the 2.1.0 line to publish `*.wasm` assets and an `srtool-digest.json`. `2.1.0-beta.1` and `2.1.0-rc.1` shipped none, and `rc.1` recorded that as a known issue blocking the final, because a governance `set_code` wants a blob whose hash is independently reproducible — not one extracted from inside a node image. The blocker was the srtool image being pinned to an older Rust (`paritytech/srtool:1.93.0-0.18.4`) than the repo builds with; upstream publishes no 1.98.x tag, so the build now uses `ghcr.io/shieldedtech/srtool:1.98.1-0.18.5`. The resulting artefact is reproducible from the recorded digest, and the first measured metadata diff for the line is in [runtime-diff.md](runtime-diff.md). *Runtime upgrade.*
**PR**: [#2166](https://github.com/midnightntwrk/midnight-node/pull/2166)

### Rust toolchain 1.98.1

**Description**: Updates Rust to 1.98.1 across the workspace `rust-toolchain.toml`, the `subxt` and nightly cNIGHT e2e container base images, and the srtool image. Two build fixes were needed: `ethnum` moves to 1.5.3 (1.5.2 transmuted `()` into `TryFromIntError`, which 1.98.1 rejects), and `WASM_BUILD_RUSTFLAGS` restores `--allow-undefined` for the runtime WASM link, which 1.98.1 dropped from the `wasm32v1-none` target spec and which Substrate's `sp_io` host-function imports rely on. New clippy lints (`useless_borrows_in_formatting`, `unnecessary_get_then_check`) and unused-import warnings account for the rest of the diff across the node, toolkit and the vendored `partner-chains` tree. *Node upgrade + Toolkit + Runtime upgrade.*
**PR**: [#2166](https://github.com/midnightntwrk/midnight-node/pull/2166), backported as [#2172](https://github.com/midnightntwrk/midnight-node/pull/2172)

### 2.1.0 benchmark weights, measured rather than estimated

**Description**: `benchmark pallet --pallet '*'` was silently skipping four surfaces, so the runtime carried estimates for them. All four are now registered and measured on the reference machine (`c7i.xlarge`, Intel Xeon Platinum 8488C — the same CPU as the 2026-05-08 run) at `STEPS=50, REPEAT=20`:

- `frame_system_extensions` — the `TxExtension` checks every signed transaction pays for. `frame_system::Config::ExtensionsWeightInfo` had been falling back to upstream's `()` impl, whose numbers are Parity's reference hardware and RocksDB; this runtime is ParityDb.
- `pallet_safe_mode` — the crate feature was enabled but the pallet was never listed, so the runtime used upstream `SubstrateWeight`. Safe mode became load-bearing when [#2079](https://github.com/midnightntwrk/midnight-node/pull/2079) made it the failed-multi-block-migration handler.
- `pallet_version` — gains a `runtime-benchmarks` feature and an `on_initialize` benchmark. Its production `WeightInfo` returned `Weight::zero()` behind a TODO while the hook appends a digest log on every block.
- `pallet_cnight_observation::set_mapping_validator_contract_address` — the one call in the pallet still priced by a hardcoded `DbWeight::writes(1)`.

Outside those, everything moved within noise except the three `pallet_cnight_observation` setters and `pallet_timestamp::on_finalize`, which fell; the cNIGHT setters were hand-written 10ms placeholders now measured at ~3.8ms. *Runtime upgrade.*
**PR**: [#2160](https://github.com/midnightntwrk/midnight-node/pull/2160)

### Bridge pallet weights corrected upward

**Description**: Every function in `pallet_c2m_bridge` and `pallet_partner_chains_bridge` rose 1.92x–2.51x. Those two weight files had previously been generated on a developer workstation (AMD Ryzen 9 9950X) rather than the reference machine, so the runtime had been budgeting roughly half the real cost on validator hardware. `handle_transfers` moves from 121µs to 233µs and is a `Mandatory` inherent, where under-weighting is the unsafe direction — a `Mandatory` call is executed regardless of the remaining block weight budget, so an under-priced one can push a block past its limit rather than being deferred. *Runtime upgrade.*
**PR**: [#2160](https://github.com/midnightntwrk/midnight-node/pull/2160)

## Deprecations

None in this delta. The `StandardTrasactionInfo` alias deprecated in `2.1.0-beta.1` is unchanged.

## Breaking changes

> A third distinct runtime blob now claims `spec_version` `002_001_000`. Operators enacting a `set_code` must reference the blob by hash, not by version — see [runtime-diff.md](runtime-diff.md) for the measured metadata and the per-hash identifiers.

### Runtime WASM changes again without a `spec_version` bump — but metadata does not

**What changed**: The runtime blob differs from `2.1.0-rc.1`'s (re-measured weights, new compiler) while `spec_version` stays `002_001_000` and `transaction_version` stays `4`. This narrows — and largely retires — the equivalent breaking change announced in `2.1.0-rc.1`, which was about metadata drifting under a frozen `spec_version`.

**What breaks**: Less than in `rc.1`. **Runtime metadata is unchanged between `rc.1` and `rc.2`, measured directly.** `rc.1` published no WASM asset, but its node image carries one, so the two blobs can be compared: `subwasm diff` reports `No change detected`, and both advertise the same 26 runtime APIs at the same versions. The checked-in `metadata/static/midnight_metadata.scale` is byte-identical at both tags (144,075 bytes) and agrees. This is expected — V14 metadata encodes types, calls, events, errors, storage and constants, **not weights**, and PR [#2160](https://github.com/midnightntwrk/midnight-node/pull/2160)'s runtime changes are `WeightInfo` associated-type swaps plus `define_benchmarks!` registrations behind `cfg(feature = "runtime-benchmarks")`. So a client, indexer or toolkit that resolved metadata against an `rc.1` chain **does not** need to be restarted before pointing it at an `rc.2` one. The `rc.1` guidance to restart such processes applies only to the `beta.1` → `rc.x` step. The reproduction is in [runtime-diff.md](runtime-diff.md).

What does still bite is artefact identity: three blobs on this line now advertise the same `spec_version`, so `spec_version` alone no longer identifies which runtime a chain is running or which one a proposal enacts.

**Required actions**:

- Identify the runtime by blob hash, not by `spec_version`. For this release: blake2-256 `0xb15f7383617c41cdb1b74b1ea4c8db0f4d0cde5e8438708a89e9f1df343a01b3`, `system.setCode` proposal hash `0x661ab01fc058cdd0bccb33b0372868eae3e6f8ef3c4263a7a63f93e6d2c5fed3`.
- Do not pin production tooling against `rc.2` metadata: `002_001_000` is not yet released onto any chain and is still being filled in on this line.
- No action for metadata consumers moving from `rc.1` to `rc.2`.

**Code example**: — (no client-facing API surface changes)

### Carried forward, unchanged

The consensus-affecting cNIGHT UTXO ordering change (PR [#2123](https://github.com/midnightntwrk/midnight-node/pull/2123)) announced in `2.1.0-rc.1` still applies in full: a `2.1.0-beta.1` binary and any `2.1.0-rc.x` binary must not validate the same `2.x` chain concurrently. That constraint is between `beta.1` and the `rc` series — **it does not apply between `rc.1` and `rc.2`**, which carry identical ordering logic. See the [`2.1.0-rc.1` notes](https://github.com/midnightntwrk/midnight-node/releases/tag/node-2.1.0-rc.1) for the full description and required actions.

## Known issues

Known issues are not freshly assessed at pre-release stage. The one item below is carried forward from `2.1.0-rc.1` because it is still true; the other `rc.1` item — no runtime WASM asset — is resolved by this release.

### Issue `Sync from genesis remains slow`

**Description**: The 16x reduction in cNIGHT observation UTXO over-fetch remains reverted for security reasons, so the follower is still on the higher Postgres round-trip volume during block import, on top of the sliding-window cache revert announced in `2.1.0-beta.1`. Nothing in this delta changes it.
**Issue**: see the [known issues board](https://github.com/midnightntwrk/midnight-node/issues?q=is%3Aissue+is%3Aopen+label%3Abug)
**Workaround (if any)**: Restore from a database snapshot rather than syncing from genesis where that option exists. The toolkit's own sync and replay path is a separate, and substantially faster, code path from node block import.

## Links and references

- **PRs**: [#2166](https://github.com/midnightntwrk/midnight-node/pull/2166), [#2172](https://github.com/midnightntwrk/midnight-node/pull/2172), [#2160](https://github.com/midnightntwrk/midnight-node/pull/2160), [#2165](https://github.com/midnightntwrk/midnight-node/pull/2165), [#2175](https://github.com/midnightntwrk/midnight-node/pull/2175), [#2161](https://github.com/midnightntwrk/midnight-node/pull/2161)
- **Engineering docs**: [Runtime diff 1.0.300 → 2.1.0-rc.2](runtime-diff.md), [Ledger 9 and the C2M bridge](../2.0.0-alpha.1/eng-ledger9-c2m-bridge.md), [Storage separation config](../2.0.0-alpha.1/config-storage-separation.md)
- **Migration guides**: [Hardfork from the 1.0.x line](../2.1.0-beta.1/migration-hardfork-1.0.x.md) — unchanged by this delta. This link resolves once PR [#2063](https://github.com/midnightntwrk/midnight-node/pull/2063), which adds the `2.1.0-beta.1` notes folder, has merged.
- **Previous releases on this line**: [node-2.1.0-rc.1](https://github.com/midnightntwrk/midnight-node/releases/tag/node-2.1.0-rc.1), [node-2.1.0-beta.1](https://github.com/midnightntwrk/midnight-node/releases/tag/node-2.1.0-beta.1), [2.0.0-alpha.1 release notes](../2.0.0-alpha.1/release-notes-2.0.0-alpha.1.md)
- **Known issues board**: [open bugs](https://github.com/midnightntwrk/midnight-node/issues?q=is%3Aissue+is%3Aopen+label%3Abug)
- **GitHub release**: [node-2.1.0-rc.2](https://github.com/midnightntwrk/midnight-node/releases/tag/node-2.1.0-rc.2)

## Fixed defect list

| Defect number | Description |
| --- | --- |
| [#2061](https://github.com/midnightntwrk/midnight-node/issues/2061) | The srtool image was pinned to an older Rust than the repo builds with, so no deterministic runtime WASM could be produced and neither `2.1.0-beta.1` nor `2.1.0-rc.1` published one. Fixed in effect by PR [#2166](https://github.com/midnightntwrk/midnight-node/pull/2166); the tracking issue is still open at the time of this release |
| [#2126](https://github.com/midnightntwrk/midnight-node/issues/2126) | The 2.1.0 weights run had not been performed, and `benchmark pallet --pallet '*'` was silently skipping four surfaces, leaving the runtime on estimates — including bridge weights generated on a developer workstation at roughly half the real cost on validator hardware (PR [#2160](https://github.com/midnightntwrk/midnight-node/pull/2160)) |
| [#1690](https://github.com/midnightntwrk/midnight-node/issues/1690) | Preview was reset in June 2026 and the regenerated chain-spec and genesis landed only on `release/node-1.0.1`; the 2.x line branched before that, so a preview node brought up from an empty disk computed genesis `0x801d…b880` instead of live preview's `0x3c096de2…6dd13796`, was rejected by every bootnode as a different chain, and sat at block #0 with no peers (PR [#2165](https://github.com/midnightntwrk/midnight-node/pull/2165)) |
| [#2159](https://github.com/midnightntwrk/midnight-node/issues/2159) | `res/devnet/` still held the pre-reset chain-spec and genesis, whose Locked pool was empty; the post-reset 1.0.300 artefacts are copied across (PR [#2175](https://github.com/midnightntwrk/midnight-node/pull/2175)) |
| [#2158](https://github.com/midnightntwrk/midnight-node/issues/2158) | `RuntimeVersion::try_from` knew `001_000_003` but not `001_000_300`, which devnet ran before the 2.1.0 hard fork, so any replay reaching a pre-fork devnet block aborted with `UnsupportedBlockVersion(1000300)` — breaking `show-night-pools`, `show-wallet` on a cold cache, and `generate-txs` without a warm one. Both versions are ledger-8 era and now share a decoder (PR [#2161](https://github.com/midnightntwrk/midnight-node/pull/2161)) |

## Other Changes

Three change fragments present on this branch have never appeared in a release note on this line, because they carry no PR number and so were missed by the `beta.1` and `rc.1` passes. They are **not** new in `rc.2` — they predate `2.1.0-rc.1` — and are listed here to close the gap:

- Fix `fork-network` runtime-upgrade mode hanging until the job timeout at "Connecting to node at ws://localhost:9950" on runners where the host-loopback to published-port path is black-holed. The `runtime` step now connects to whichever RPC endpoint actually answers, preferring the published port and falling back to node1's docker bridge IP, and `createApi` fails fast after a bounded `API_CONNECT_TIMEOUT_MS`. `full` mode still relies on the published port and needs a follow-up tooling change.
- Use `apply_post_block_update` in `pallet_midnight`'s `on_finalize`, which has one fewer theoretical failure mode. In practice the removed error could not occur, since block fullness is checked for each included transaction.
- Clear the backlog of pending major dependency updates: `actions/cache` v6.1.0 and `azure/setup-kubectl` v5.1.0 (both commit-pinned), Docker Compose v5.5.0 plus the buildx plugin it now delegates `build:` to, the local-env contract-compiler base on `node:24-slim`, eslint v10, and `@types/node` 24 in `util/toolkit-js`.
