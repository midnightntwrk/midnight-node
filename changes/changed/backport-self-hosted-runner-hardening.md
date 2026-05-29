# Backport self-hosted runner hardening to release/node-1.0.1

Backports four CI-side fixes from `main` that harden the self-hosted
GitHub Actions runner pool against shared-host state leakage:

- Migration of paid x86_64 CI workflows to the self-hosted runner pool
  (originally #1483).
- Per-slot docker config isolation so concurrent jobs on the same
  runner host don't trample each other's Docker auth (originally
  #1585).
- Sync-test and local-env hardening against shared-host state —
  unique container names, scoped temp dirs (originally #1593).
- Scoped Earthly `/target` cache by a CI-derived key
  (`EARTHLY_GIT_BRANCH` is unsafe under `actions/checkout` detached
  HEAD; instead the workflow now derives `CACHE_KEY` from the PR head
  ref and passes it as a build-arg to `+test`, `+test-pallet-fixtures`
  and `+test-toolkit`). Prevents `.rmeta` / `.rlib` artifacts from one
  PR poisoning another's build on the same host.

No code or runtime changes; CI / workflow / Earthfile / runner-config
only.

PR: https://github.com/midnightntwrk/midnight-node/pull/1610
Issue: https://github.com/midnightntwrk/midnight-node/issues/1599
