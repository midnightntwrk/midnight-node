# Vendored dependency tarballs

This directory holds the `npm pack` tarballs that the `compact-0.31.108` variant consumes via
`file:` references in its `package.json`. They are **not committed by hand** — they are built from
the pinned `midnight-sdk` submodule and committed back by the
`rebuild-compact-js-bundle-bot` workflow (a verified `github-actions[bot]` commit).

Expected blobs (stable filenames, regardless of the internal package versions):

| File | Source | Why vendored |
|---|---|---|
| `compact-js.tgz` | `midnight-sdk` packages (compact-js 2.5.3) | unpublished (pinned SDK commit) |
| `compact-js-command.tgz` | `midnight-sdk` packages (compact-js 2.5.3) | unpublished (pinned SDK commit) |
| `compact-js-node.tgz` | `midnight-sdk` packages (compact-js 2.5.3) | unpublished (pinned SDK commit) |
| `compact-runtime.tgz` | `midnight-sdk/compact-submodule/runtime` (built via nix) | nowhere published for this era |
| `ledger-v9.tgz` | `@midnight-ntwrk/ledger-v9@0.1.0-alpha.1` (GitHub Packages) | private registry |

Everything else in the dependency closure resolves from public npm, so the consume path
(`npm ci` in `util/toolkit-js`) needs no `.npmrc` and no token.

## Regenerating

Trigger the bot on the PR:

```
/bot rebuild-compact-js-bundle
```

or locally (requires nix + a GitHub Packages read token):

```
earthly -P +compact-js-bundle --secret MIDNIGHTCI_PACKAGES_READ=<token>
```

See `scripts/build-compact-js-bundle.sh` and the `+compact-js-bundle` target in `Earthfile`.
