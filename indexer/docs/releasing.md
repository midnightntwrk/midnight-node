# Creating a Release

How a versioned release of the indexer is cut and published.

## Versioning

The workspace shares one [SemVer](https://semver.org/) version, set once in the
root `Cargo.toml` (`[workspace.package] version`); crates inherit it via
`version.workspace = true`.

## Changelog

`CHANGELOG.md` is grown by [git-cliff](https://git-cliff.org/) from
conventional-commit messages, per `cliff.toml` (which also skips `chore(release)`,
`chore(deps*)`, and `test`). **Commit messages must be conventional** (`feat:`,
`fix:`, `chore(...):`, ...) or they are dropped.

The file is **append-only**: each release *prepends one section* for the
unreleased range rather than regenerating (early entries predate the current
`cliff.toml` and won't reproduce). The command:

```bash
git cliff --unreleased --tag vX.Y.Z --prepend CHANGELOG.md
```

Review the prepended section before committing.

## Cutting a release

1. **Prepare PR**, titled `chore(release): prepare for X.Y.Z`: bump `version` in
   the root `Cargo.toml` (let `Cargo.lock` follow) and prepend the changelog
   section (above). The diff is a `version` bump plus a *pure addition* atop
   `CHANGELOG.md` - copy the last `chore(release)` PR as the template. Review,
   merge.

2. **Tag** the merge commit with an **annotated, GPG-signed** tag and push (releases are
   signed; a lightweight `git tag vX.Y.Z` is not enough):

   ```bash
   git tag -s -a vX.Y.Z -m "Release X.Y.Z"   # on the merge commit
   git verify-tag vX.Y.Z                       # confirm the signature
   git push origin vX.Y.Z
   ```

   Confirm the tag sits on the merge commit that is the remote branch tip before pushing.

3. **Images publish automatically.** A `v*` tag triggers
   `.github/workflows/build-indexer-images.yaml`, which builds every component
   (`chain-indexer`, `wallet-indexer`, `indexer-api`, `spo-indexer`,
   `indexer-standalone`) with the `release` profile and pushes semver-tagged
   images to `ghcr.io/midnight-ntwrk/<component>` (always) and
   `docker.io/midnightntwrk/<component>` (tag builds only). The same workflow also runs on
   **main pushes** and **manual dispatch**, tagging those images `<cargo-version>-<short-sha>`
   with the `dev` profile (GHCR only) - a pre-merge or feature image is identifiable by its
   `-<sha>` suffix.

## Scheduling & sign-off

There is no fixed cadence - releases are on-demand and gated: **QA signs off the release
candidate** and the **maintainers make the release call**, with prod / mainnet timing gated by the
wider release schedule. Feature-integration (pre-alpha) images are cut whenever a meaningful chunk
lands on the integration branch.

## Maintenance branches

Fixes for a shipped line live on `release/*` branches (e.g. `release/4.3.1`); CI
runs on them as on `main`.

## Pre-release / dev tags

**Release candidates** `vX.Y.Z-rc.N` are cut on a `chore/release-X-Y-Z` branch and shipped to QA
ahead of the final `vX.Y.Z`; they build with the `release` profile like a real release.

Feature-integration builds use non-semver tags encoding the ledger/node RCs they were
built against:

```text
v4.4.0-pre-alpha.14-l91r3-n2r3-bridge-and-events-epics-ca3e554
                    ^^^^^ ^^^^                        ^^^^^^^
                  ledger  node                        commit
```

These never reach Docker Hub - only semver-pattern tag builds get the
`midnightntwrk/*` images and `latest`.

## See also

- [Testing & node consistency](./testing.md)
- [Upgrading the node version](./updating-node-version.md)
- [Upgrading the ledger](./upgrading-ledger.md)
