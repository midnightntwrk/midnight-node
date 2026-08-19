#toolkit #ci #local-env
# Clear the backlog of pending major dependency updates

Takes seven of the nine major updates queued behind manual approval on the
Renovate dependency dashboard, and records why the other two are not takeable.

CI base image: `actions/cache` to v6.1.0 and `azure/setup-kubectl` to v5.1.0,
both pinned by commit. Docker Compose moves to v5.5.0, which required installing
the buildx plugin alongside it - compose v5 removed its internal buildkit
builder and delegates `build:` to Docker Bake, so without buildx the
contract-compiler service in local-env's compose file no longer builds.

The `paritytech/srtool` "v1" offer was a false positive: published tags are
`<rust version>-<srtool version>`, and splitting that across an ARG and a
hard-coded prefix made Renovate read the rust half as the image version and
offer a tag that does not exist. The whole tag is now one ARG, so deterministic
runtime builds keep resolving.

Local environment: the contract-compiler base moves to `node:24-slim` (still
Debian bookworm, so the pinned apt package set is unaffected), and eslint moves
to v10. eslint 10 drops `@eslint/eslintrc`, which removes js-yaml from the tree
entirely - the only way the js-yaml major was ever going to resolve, since
eslintrc caps it at 4.x. Two things fell out: `globals` was a phantom dependency
that only resolved through eslint 9's transitive tree, and eslint 10's
`preserve-caught-error` rule caught a config-parse failure being re-thrown with
the original error discarded, which now attaches it as `cause`.

Toolkit: `@types/node` moves to 24 in `util/toolkit-js`, which already targeted
`@tsconfig/node24`. The redundant `nanoid` and `js-yaml` `overrides` entries are
removed - both were npm-audit remediations whose consumers have since raised
their own ranges, so they pinned exactly what npm would pick unaided, and
removing them leaves the lockfiles byte-identical.

Not taken. TypeScript 7 is blocked everywhere in the tree, not just where eslint
runs: `@midnight-ntwrk/compact-js-command` depends on ts-node and compiles
`contract.config.ts` at runtime, and ts-node drives the JavaScript compiler API
that the native port does not expose, so toolkit contract deploys and genesis
rebuilds fail with `Cannot read properties of undefined (reading 'fileExists')`.
In `local-environment` typescript-eslint independently caps `typescript` below
6.1. `postgres` v18 spans four db-sync-adjacent pins and needs its own change.

PR: https://github.com/midnightntwrk/midnight-node/pull/2047
