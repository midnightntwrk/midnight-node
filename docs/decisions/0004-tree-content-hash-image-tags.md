# 0004: Tree Content Hash for Docker Image Tags

**Date:** 2026-02-25
**Status:** Proposed
**Deciders:** @gilescope

## Context

Docker images are tagged `{VERSION}-{8-char-commit-hash}-{ARCH}`. Every push to `main` triggers a full rebuild even when the tree content is unchanged. This happens because commit hashes change with every commit (different timestamp, parent, message) even if the actual source tree is identical. Merge commits, reverts-of-reverts, and cherry-picks all produce different commit hashes for the same tree.

A full node+toolkit build takes significant CI time and resources. Rebuilding identical binaries wastes compute and delays downstream consumers.

## Decision

Replace the 8-char commit hash with a 12-char tree content hash (`git rev-parse HEAD^{tree} | cut -c1-12`) in image tags.

New tag format: `{VERSION}-{12-char-tree-hash}-{ARCH}`

### Properties of tree hashes

- Two commits with identical file trees produce the same tree hash
- Any file change (content, permissions, additions, deletions) produces a different tree hash
- Tree hashes are deterministic and reproducible across clones

### Implementation

| Component | Change |
| --------- | ------ |
| `season-action` | New `hash_type` input (`commit`/`tree`), defaults to `commit` for backward compatibility |
| `Earthfile` | Image targets compute `CONTENT_HASH` LET instead of `EARTHLY_GIT_SHORT_HASH` |
| `main.yml` | Computes tree hash, checks if images exist before building, `force_rebuild` input to override, creates commit-hash alias tags unconditionally |

### Skip logic

Before building, CI checks whether both `midnight-node:{TAG}-{ARCH}` and `midnight-node-toolkit:{TAG}-{ARCH}` already exist in GHCR. If both exist and `force_rebuild` is not set, the build is skipped. Signing runs unconditionally (idempotent).

### Commit-hash alias tags

Every CI run — whether it builds or skips — also creates alias tags in the old `{VERSION}-{8-char-commit-hash}-{ARCH}` format pointing to the same image. This restores commit-to-image traceability without sacrificing content-hash deduplication.

```text
                content hash (primary, dedup)
                        +----------+
  commit abc123 --tag-->|          |
  commit def456 --tag-->|  image   |<-- tag -- 0.20.0-abc123def0-amd64
  commit 789abc --tag-->|  digest  |
                        +----------+
```

- Forward lookup: pull by commit hash tag, Docker resolves to the content-hash image
- Reverse lookup: enumerate tags sharing the same digest (via GHCR API) to find all commits that produced a given image
- Alias tags are created via `docker buildx imagetools create --tag` (no rebuild, no re-push of layers)
- Multi-arch commit-hash manifests are created in the `publish-multi-arch` job

### `GIT_CONTENT_HASH` environment variable

Every image embeds the full 40-char tree hash as `GIT_CONTENT_HASH`. To find all commits that produced a given image:

```bash
git log --all --format='%h %T' | grep $(docker run --rm --entrypoint printenv midnightntwrk/midnight-node:latest-main GIT_CONTENT_HASH)
```

## Alternatives Considered

| Option | Description | Decision |
| ------ | ----------- | -------- |
| **Tree hash (12-char)** | Content-addressed tags, skip redundant builds | **Selected** |
| Commit hash (status quo) | Every push rebuilds | Rejected - wasteful |
| Path-based change detection | `paths-filter` on workflow triggers | Rejected - fragile with transitive deps, doesn't handle cherry-picks |
| Cargo.toml version only | Tag by semver alone | Rejected - can't distinguish dev builds within a version |

### Why 12 characters?

- 8 chars (32 bits) = ~1 in 4 billion collision chance per pair, but birthday bound is ~65k objects
- 12 chars (48 bits) = birthday bound of ~16 million objects, ample for image tags
- Differentiates from the old 8-char commit hashes, making it visually obvious which scheme is in use

## Consequences

### Positive

- Identical trees skip builds entirely, saving CI time and compute
- Cherry-picks and merge commits that don't change content reuse existing images
- `force_rebuild` input provides an escape hatch
- Backward compatible: `season-action` defaults to `commit` hash for other consumers

### Negative

- Content-hash tags are not traceable to a specific commit (multiple commits may share a tree hash) — mitigated by commit-hash alias tags
- 12-char hashes are longer than the previous 8-char ones

### Not Changed

- `build-prepare` still uses `EARTHLY_GIT_SHORT_HASH` for `SUBSTRATE_CLI_GIT_COMMIT_HASH` (embedded in the binary, different purpose)
- `partnerchains-dev` still uses `EARTHLY_GIT_SHORT_HASH` (separate lifecycle)
- Indexer images still use `via-node-{commit}` (different purpose)
- `continuous-integration.yml` unchanged (separate PR)
- `release-image.yml` unchanged (will work once season-action is updated and re-pinned)

## References

- `main.yml` - Main build/publish workflow
- `Earthfile` - Image target definitions
- `season-action/action.yml` - Release variable computation
- `git rev-parse` [docs](https://git-scm.com/docs/git-rev-parse) - `HEAD^{tree}` dereferences to the tree object
