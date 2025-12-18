# 0004: Fix Node Image Version Tag to Preserve Semver Pre-release Suffix

**Date:** 2025-12-18  
**Status:** Accepted  
**Deciders:** @m2ux

## Context

Docker image version tags are generated inconsistently across Earthfile build targets. The toolkit image correctly preserves semver pre-release suffixes (e.g., `0.19.0-rc.1`), while the node image strips them (producing `0.19.0` instead).

This creates problems:
1. **Inconsistent tagging** - Node and toolkit images have different version formats for the same build
2. **Broken workflows** - Downstream consumers expecting consistent version tags fail to match images
3. **Release confusion** - Pre-release versions appear as stable releases in registries

## Decision

Align node image version extraction with the proven toolkit image approach by reading the version directly from the canonical source (Cargo.toml) rather than parsing binary output.

## Alternatives Considered

| Option | Description | Decision |
|--------|-------------|----------|
| **Read from canonical source** | Use same approach as toolkit image | **Selected** - proven, consistent |
| Fix output parsing | Parse binary output with smarter regex | Rejected - fragile, depends on output format |
| Do nothing | Accept inconsistent tags | Rejected - causes downstream issues |

## Consequences

### Positive
- All image tags use consistent version format including pre-release suffixes
- Simpler, more maintainable version extraction
- Aligns with existing working pattern

### Negative
- None identified

### Neutral
- Requires version source file to be available in the image build context (already satisfied)
