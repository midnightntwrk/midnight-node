#local-env #dependencies
# Drop the unused `wait-on` dependency and bump Node.js to 24.22.2

`wait-on` and `@types/wait-on` were declared in `local-environment/package.json` but never imported under `src/`. Removing them also clears the high findings `npm audit` reports, which reach the dependency tree only through `wait-on`.

`NODEJS_VERSION` moves from 24.18.0 to 24.22.2, picking up the October 2026 Node.js security release. The `fork-network` workflow's `setup-node` pin moves with it.

PR: https://github.com/midnightntwrk/midnight-node/pull/2355
Issue: https://github.com/midnightntwrk/midnight-node/issues/2354
