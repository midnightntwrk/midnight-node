#local-env #dependencies
# Drop the unused `wait-on` dependency from local-environment

`wait-on` and `@types/wait-on` were declared in `local-environment/package.json` but never imported under `src/`. Removing them also clears the high findings `npm audit` reports, which reach the dependency tree only through `wait-on`.

PR: https://github.com/midnightntwrk/midnight-node/pull/2355
Issue: https://github.com/midnightntwrk/midnight-node/issues/2354
