# 0003: ARM64 CI Builds for PR and Merge Queue Pipelines

**Date:** 2025-12-16  
**Status:** Accepted  
**Deciders:** @m2ux

## Context

ARM64 Docker images are currently only built during merge commits to `main` and `release/*` branches (via `main.yml`). The `continuous-integration.yml` workflow, which runs for pull requests and merge queue events, only builds AMD64 images.

This creates two problems:
1. **Late integration feedback:** Dependent components cannot receive ARM64 images for integration testing until code is merged to main
2. **No pre-merge validation:** ARM64-specific issues are only discovered after merge, requiring follow-up fixes

## Decision

Enable ARM64 builds in `continuous-integration.yml` with **label-based gating** for PRs:

| Event | AMD64 | ARM64 |
|-------|-------|-------|
| `pull_request` (no label) | ✅ Build | ❌ Skip |
| `pull_request` + `ci:arm64` label | ✅ Build | ✅ Build |
| `merge_group` | ✅ Build | ✅ Build |
| `workflow_dispatch` | ✅ Build | ✅ Build |

### Implementation Details

1. **Uncomment ARM64 matrix entry** in the `run` job's strategy matrix
2. **Add job-level conditional** using `contains(github.event.pull_request.labels.*.name, 'ci:arm64')`
3. **Add QEMU setup step** (conditional on ARM64) for compactc compatibility
4. **Fix artifact naming** to include architecture suffix, preventing upload collisions

## Alternatives Considered

| Option | Description | Decision |
|--------|-------------|----------|
| Always build both | Run ARM64 for all PRs | Rejected - doubles CI cost/time for all PRs |
| Merge queue only | Only build ARM64 at merge queue | Rejected - no early ARM64 for integration |
| **Label-gated** | ARM64 on-demand via label, mandatory at merge queue | **Selected** |

## Consequences

### Positive
- Dependent components can add `ci:arm64` label to get early ARM64 images for integration testing
- Merge queue always validates ARM64, preventing broken ARM64 builds from reaching main
- Default PRs remain fast (AMD64 only)
- Consistent with existing `main.yml` ARM64 build pattern

### Negative
- Requires manual label addition when ARM64 needed early
- Slight increase in workflow complexity

### Neutral
- ARM64 builds run in parallel with AMD64; total time = max(amd64, arm64)
- No changes required to `main.yml` or release workflows

## References

- `main.yml` - Existing ARM64 build implementation (lines 225-270)
- `continuous-integration.yml` - Target workflow for modification

