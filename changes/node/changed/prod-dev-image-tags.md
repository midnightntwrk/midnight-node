#node #toolkit #ci

# Tag cached (non-hermetic) CI node/toolkit images with a distinct `-dev` tag

Cached, non-hermetic Earthly builds (`PROD=false`, used by PR and merge-group CI)
previously published node/toolkit images under the same clean canonical tag
`<version>-<treehash>-<arch>` that hermetic release builds (`PROD=true`, used by
main / `release/*` pushes) use, so a cached build could occupy or "mask" a release
image's tag. This was worked around with a separate `-hermetic-` marker tag.

Now the tag itself encodes the build mode: hermetic (`PROD=true`) builds publish the
clean canonical tag (and move `latest-<arch>`), while cached (`PROD=false`) builds
publish only a `-dev` tag `<version>-dev-<treehash>-<arch>` (private and public
registries). Because the clean tag can now only ever come from a hermetic build, its
existence proves hermeticity, so the `-hermetic-` marker and the dedup logic keyed off
it have been removed. CI consumers (e2e, SBOM, genesis, local-env) resolve the `-dev`
tags via the workflow job outputs, so no consumer changes were needed.

PR:
Issue:
