#toolkit #compactc #build
# Update CompactC to 0.33.0-rc.0 (compact-js 2.5.5-rc.4)

Bumped `COMPACTC_VERSION` from the `0.31.110` dev build to `0.33.0-rc.0` and repointed
the `midnight-sdk` submodule at `compact-js-v2.5.5-rc.4`. `0.33.0-rc.0` is a semver
pre-release, so `+node-ci-image` fetches the prebuilt `compactc-v0.33.0-rc.0` release
rather than building from the `compact/` submodule.

The vendored `compact-js` variant is renamed `compact-0.31.110/` → `compact-0.33.0/`
(the resolver dispatches on the `<major>.<minor>.<patch>` = `0.33.0`), pinning
`@midnight-ntwrk/compact-js*` 2.5.5-rc.4 and `@midnight-ntwrk/compact-runtime`
0.18.0-rc.0 from freshly rebuilt `file:` tarballs. The variant's `@effect/platform-node`
range is bumped `^0.106.0` → `^0.107.0` (compact-js-command 2.5.5-rc.4 requires it) and
`effect` `^3.21.2` → `^3.21.4`. `SUPPORTED_COMPACTC_VERSIONS`, the root workspace
dependency, `test-all-compactc.sh`, the `+compact-js-bundle` Earthly target,
`build-compact-js-bundle.sh`, and the `rebuild-compact-js-bundle-bot` workflow are all
updated to the new variant path.

PR: <link to PR>
Issue: <link to Github Issue, if applicable>
