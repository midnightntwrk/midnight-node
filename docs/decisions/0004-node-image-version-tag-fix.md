# 0004: Fix Node Image Version Tag to Preserve Semver Pre-release Suffix

**Date:** 2025-12-18  
**Status:** Proposed  
**Deciders:** @m2ux

## Context

Docker image version tags are generated inconsistently across the Earthfile build targets:

- **`toolkit-image`** correctly uses `0.19.0-rc.1-<hash>-<arch>` by reading directly from `node/Cargo.toml`
- **`node-image`** incorrectly uses `0.19.0-<hash>-<arch>` (missing `-rc.1`) due to flawed awk parsing

The root cause is in the version extraction command used by `node-image` and `node-benchmarks-image`:

```bash
./midnight-node --version | awk '{print $2}' | awk -F- '{print $1}'
```

The `awk -F- '{print $1}'` splits the version string by `-` and takes only the first segment, discarding any semver pre-release suffix like `-rc.1`.

This creates problems:
1. **Inconsistent tagging** - Node and toolkit images have different version formats for the same build
2. **Broken workflows** - Downstream consumers expecting consistent version tags fail to match images
3. **Release confusion** - Pre-release versions appear as stable releases in registries

## Decision

Align `node-image` and `node-benchmarks-image` version extraction with the proven `toolkit-image` approach:

| Target | Current Method | New Method |
|--------|----------------|------------|
| `node-image` | Parse `--version` output with awk (broken) | Read from `node/Cargo.toml` |
| `node-benchmarks-image` | Parse `--version` output with awk (broken) | Read from `node/Cargo.toml` |
| `toolkit-image` | Read from `node/Cargo.toml` | No change (already correct) |

### Implementation

Replace the flawed command:
```bash
RUN ./midnight-node --version | awk '{print $2}' | awk -F- '{print $1}' | head -1 > /version
```

With the working pattern already used by toolkit-image:
```bash
RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > /version
```

## Alternatives Considered

| Option | Description | Decision |
|--------|-------------|----------|
| **Read from Cargo.toml** | Use same approach as toolkit-image | **Selected** - proven, consistent |
| Fix awk regex | Parse `--version` output with smarter regex | Rejected - fragile, depends on output format |
| Do nothing | Accept inconsistent tags | Rejected - causes downstream issues |

## Consequences

### Positive
- All image tags use consistent version format including pre-release suffixes
- Simpler, more maintainable version extraction
- Aligns with existing working pattern in toolkit-image

### Negative
- None identified

### Neutral
- Requires `node/Cargo.toml` to be available in the image build context (already satisfied)

## References

- `Earthfile` - Lines 862-869 (node-image), Lines 895-901 (node-benchmarks-image)
- `images/toolkit/Dockerfile` - Lines 21-24 (working reference implementation)

