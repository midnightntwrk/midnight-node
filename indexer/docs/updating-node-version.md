# Upgrading the Node Version

How to add or move to a Midnight Node version the indexer talks to.

## Model: many versions at once

`NODE_VERSIONS` (one version per line, oldest first, **append-only**) lists
every node version this build supports at once; the last line is the "latest"
for local `just` recipes. The indexer picks a runtime per block from the chain's
protocol version (`indexer-common/src/domain/protocol_version.rs`).

Per listed version `X`:

- `.node/X/metadata.scale` - subxt metadata, consumed at build time.
- `.node/X/chain/...` - paritydb snapshot for tests.
- `chain-indexer/src/infra/subxt_node/runtimes/vX.rs` - version-specific decode
  logic.

`build.rs` reads `NODE_VERSIONS` and emits a `#[subxt::subxt(...)]` module per
entry from its `metadata.scale`; a missing file fails the build.

## Adding / bumping a version

### 1. Record the version

Append the version to `NODE_VERSIONS`. An RC roll-forward of an existing line
(`2.0.0-rc.1` -> `rc.3`) just edits that last line.

### 2. Generate data + metadata

```bash
just update-node    # generate-node-data + get-node-metadata for the latest line
```

Produces `.node/X/chain/` (snapshot) and `.node/X/metadata.scale`. Needs `subxt`
at the version pinned in `Cargo.toml`: `cargo install subxt-cli --version
<pinned>`.

### 3. Wire the runtime (new protocol version only)

If the version is new (not an RC of an existing line):

- add `runtimes/vX_Y_Z.rs` (copy the nearest existing one),
- register it and add its match arms in `runtimes.rs`,
- extend `NodeVersion`, the `ProtocolVersion` ranges, and the mappings in
  `protocol_version.rs`.

An RC bump needs none of this - the generated subxt module re-derives from the
new metadata, unless pallets/events changed shape.

### 4. Regenerate tx fixtures (if the wire format moved)

```bash
just generate-txs   # rewrites indexer-common/tests/*.raw from a running node
```

### 5. Drop superseded data (optional)

Delete `.node/<old-version>/` once it is off `NODE_VERSIONS`.

### 6. Verify

```bash
just all-all
just run-node                          # latest NODE_VERSIONS line
cargo test -p indexer-tests native_e2e
```

If a test hardcodes the old version (a version string, block hash, or count),
find it by searching rather than trusting a fixed location - such tests have
moved before: `rg '0\.22\.0|<old-version>'`.

### PR checklist

- [ ] `NODE_VERSIONS` updated
- [ ] `.node/<version>/{metadata.scale,chain/}` present
- [ ] `just all-all` green
- [ ] no references to a removed version (`rg <old-version>`)

A **new protocol version** (not an RC roll-forward) also needs:

- [ ] `runtimes/vX_Y_Z.rs` plus dispatch arms in `runtimes.rs`
- [ ] `protocol_version.rs`: new `NodeVersion` variant, `ProtocolVersion` range,
      and `→ NodeVersion` / `→ LedgerVersion` mappings

## Breaking changes

A node bump can move the runtime API:

| Symptom | Cause | Fix lives in |
| ----------------------------------- | --------------------- | ------------------------------------------------------- |
| `E0560` struct has no field         | field removed/renamed | `indexer-common/src/domain/`, GraphQL schema if exposed |
| missing/extra fields on destructure | event struct changed  | `chain-indexer/.../runtimes/vX.rs`                      |
| hex/decode runtime error            | tx encoding changed   | `chain-indexer/src/infra/subxt_node/`                   |

Exposed field changed → regen the GraphQL schema
(`just generate-indexer-api-schema`). *Stored* field changed → add a migration
under `indexer-common/migrations/`. A protocol bump usually rides with a ledger
bump - see [Upgrading the ledger](./upgrading-ledger.md).

## Common mistakes

- **Metadata without code** - a new protocol version needs its runtime module
  and dispatch arms, not just `metadata.scale`.
- **Stale inline test data** - green locally if you skip tests, red in CI.
- **Partial search** - `rg <old-version>` catches every occurrence; eyeballing
  misses some.
- **No live node** - `just all-all` alone won't surface wire-format mismatches.

## CI considerations

CI fails if a listed version's `metadata.scale` is missing, versions disagree
across `NODE_VERSIONS` / code / `.node/`, or a test points at a deleted `.node/`
directory.

## Rollback

Revert the PR; the new `.node/<version>/` data can stay (harmless). Confirm
`NODE_VERSIONS` and code point back at the known-good version.

## See also

- [Upgrading the ledger](./upgrading-ledger.md)
- [Creating a release](./releasing.md)
