# Vendored dependency tarballs

This directory holds the `npm pack` tarballs that the `compact-0.31.108` variant consumes via
`file:` references in its `package.json`. They are the four `@midnight-ntwrk` packages in the
compact-js 2.5.3 closure that are **only published to GitHub Packages** (a private registry);
everything else in the closure resolves from **public npm**, so the consume path
(`npm ci` in `util/toolkit-js`) needs no `.npmrc` and no token.

| File | Source | Why vendored |
|---|---|---|
| `compact-js.tgz` | `@midnight-ntwrk/compact-js@2.5.3` | GitHub Packages only |
| `compact-js-command.tgz` | `@midnight-ntwrk/compact-js-command@2.5.3` | GitHub Packages only |
| `compact-js-node.tgz` | `@midnight-ntwrk/compact-js-node@2.5.3` | GitHub Packages only |
| `compact-runtime.tgz` | `@midnight-ntwrk/compact-runtime@0.16.103-dev.<hash>` | GitHub Packages only |

Resolved from public npm (not vendored): `@midnightntwrk/ledger-v9@^1.0.0-rc.2`,
`@midnightntwrk/onchain-runtime-v4`, `@midnight-ntwrk/platform-js@^2.2.4`,
`@midnight-ntwrk/wallet-sdk-address-format@3.1.0`, `@midnight-ntwrk/ledger-v8@8.0.2`, and the
`@effect/*` packages.

## How these were produced

The compact-js 2.5.3 packages aren't published with this SDK revision's dep set, so the three
`compact-js*` blobs are built from the pinned `midnight-sdk` submodule. Pack them via the SDK's own
`package` step (build-utils), not a raw `npm pack` of the source dir — the source `package.json`
`exports` point at `./src/*.ts`, whose `.d.ts` don't line up with `@effect/cli`'s `Command` type in a
consumer (a `[TypeId]` mismatch). The `dist/` layout the `package` step emits matches the published
packages:

```
cd midnight-sdk/compact-js
corepack yarn install          # needs a GitHub Packages read token for the @midnight-ntwrk scope
corepack yarn package          # turbo: build (build-esm + build-utils pack-v3) then npm pack from dist/
# copy each of compact-js/dist/*.tgz, compact-js-command/dist/*.tgz, compact-js-node/dist/*.tgz here
```

`compact-runtime.tgz` is `npm pack`ed straight from GitHub Packages
(`@midnight-ntwrk/compact-runtime@<version>`, the version compact-js depends on) — it is a published
dev build, **not** built from source. (Plain TS throughout — no nix, no compact-submodule.)

This whole flow is automated by the `+compact-js-bundle` Earthly target
(`scripts/build-compact-js-bundle.sh`) and the `rebuild-compact-js-bundle-bot` workflow
(`/bot rebuild-compact-js-bundle`). The GitHub Packages read token is needed **only** to build/pack
these blobs — never on the consume path.
