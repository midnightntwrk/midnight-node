# 0004: Tree Content Hash for Docker Image Tags

**Date:** 2026-02-25
**Status:** Amended: tree reuse applies to PR CI; main/release images use commit identity
**Deciders:** @gilescope

## Release provenance amendment

Node and toolkit images built by `main.yml` use
`{NODE_VERSION}-{full-40-character-commit-SHA}-{ARCH}`. Both architectures and the
multi-architecture manifest resolve the requested ref once, so a moving branch
cannot mix different commits. PR CI retains tree-based tags and deduplication.

The binary embeds the abbreviated build commit through
`SUBSTRATE_CLI_GIT_COMMIT_HASH`. Reusing a PR test-merge image after the final merge
preserves that earlier commit in telemetry, even when both commits have identical
source trees. A commit alias alone cannot change the embedded binary version.

Main builds pass the commit SHA to Earthly as `IMAGE_TAG_HASH`; ordinary local and
PR builds retain the tree-hash default. Node and toolkit images record the full
build commit in `org.opencontainers.image.revision`. Existing images are reused
only when that label matches the requested commit for the target architecture.
The tree hash remains available as `GIT_CONTENT_HASH` for content comparisons.

`release-image.yml` resolves `inputs.ref` to `RELEASE_SHA`, selects the corresponding
commit-tagged images, and checks their revision labels for both architectures
before publishing any release image. The private manifest and the public mirror
are checked separately. Missing images, missing labels, and mismatched revisions
fail the release with a request to run Main build/publish for the exact commit.
Node/toolkit skip flags and the runtime-only release path remain supported.

GHCR and Docker Hub expose commit-specific tags alongside the release tags.
Version-only tags such as `1.0.400`, architecture tags, and release archive names
are unchanged. Release archives are extracted from the promoted images, so they
carry the same binary build identity. This change does not rewrite existing
releases. Release branches need the updated build workflow and Earthfile before
they can supply commit-specific release artifacts; legacy tree aliases are not a
fallback. Different commits with the same tree now require separate main builds.

The original tree-reuse decision below describes PR CI and the previous main
release behavior; this amendment supersedes its main/release-specific claims.

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
