# Backport self-hosted runner hardening to release/node-1.0.1

Backports four CI-side fixes from `main` that harden the self-hosted
GitHub Actions runner pool against shared-host state leakage. No
runtime, node, or toolkit code is touched — workflow / Earthfile /
runner config / one e2e script only.

- **#1483** — migrate paid x86_64 CI workflows to the self-hosted
  runner pool. Drops the GitHub-hosted spend on the largest jobs and
  brings the release branch's runner usage in line with main.
- **#1585** — isolate docker config per self-hosted runner slot.
  Concurrent jobs on the same host were stepping on each other's
  Docker auth; this scopes `DOCKER_CONFIG` to `$RUNNER_TEMP/.docker`
  via `$GITHUB_ENV` so every job (run, test-toolkit,
  mainnet-sync-test, local-environment-tests) gets an isolated
  config that `docker/login-action` writes to and earthly reads from.
- **#1593** — harden self-hosted sync-test and local-env against
  shared-host state. Unique container names per job and pre-flight
  teardown of the local-env compose project (by label rather than
  hand-maintained port list) so leftover containers from a cancelled
  or crashed prior run don't block the fresh stack.
- **Earthly `/target` cache scoped by CI-derived key**
  (`fix(ci): scope earthly /target cache by an explicit CI-derived
  key`). `EARTHLY_GIT_BRANCH` was unsafe under `actions/checkout`'s
  detached-HEAD checkout (resolved to the literal `HEAD` for every
  PR); the workflow now derives `CACHE_KEY` from `github.head_ref`
  / `github.ref_name`, sanitizes via `tr`, and passes it as a
  build-arg to `+test`, `+test-pallet-fixtures`, and `+test-toolkit`.
  Prevents `.rmeta` / `.rlib` from one PR poisoning another's build
  on the same self-hosted host.

PR: https://github.com/midnightntwrk/midnight-node/pull/1611
Issue: https://github.com/midnightntwrk/midnight-node/issues/1599
