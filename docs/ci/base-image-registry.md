# CI base image registry

The main PR build, core-test and lint workflows select the existing private
`ghcr.io/midnight-ntwrk/midnight-node-ci` copy when running in the canonical
repository with registry credentials. Preparation and dependency-check jobs
authenticate before resolving that base too. Core tests use the existing CI
account's write credential so remote-cache publication remains available.

Fork PRs, clones, jobs without credentials and local builds retain the public
`midnightntwrk/midnight-node-ci` Docker Hub default. `NODE_CI_IMAGE_REPO` overrides
the repository only; Rust, Compact, npm and architecture still select the same
version tag. `.envrc` forwards the override to EarthBuild. Publication targets
are unchanged.

The contract-precompiles producer publishes the GHCR image first and copies it
to Docker Hub. Moving authenticated readers avoids Hub manifest requests for
this large custom image, including the 429 failures seen despite cached layers.
Other workflows keep the public default unless they explicitly opt in.

Validate anonymous/fork defaults with `scripts/ci/test_ci_base_registry.py`.
Validate authenticated pulls in CI logs on both architectures, then inspect
the complete core, toolkit and local-environment test results. Rollback is
removing the workflow override; no runner or shared BuildKit configuration changes.
