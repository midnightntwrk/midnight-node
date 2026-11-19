# CI local-environment image loading

`+local-env-ci` starts a fresh nested Docker daemon for each stack. The host's
Docker image store is separate from that daemon. Images needed by the stack are
therefore loaded through `WITH DOCKER --pull`, which reuses the shared BuildKit
image cache before transferring layers into the nested daemon.

The public image references are resolved with Docker Compose from the same
defaults and private-image overrides used to start the `withindexer` stack.
`local-environment/src/lib/ciImages.ts` checks that every active image-backed
service is accounted for. Build-only services are built by Compose; their
Dockerfile base images are not part of the service image preload list.

When adding an image-backed CI dependency, add its service to the mapping in
`ciImages.ts` and its corresponding `--pull` argument in `+local-env-ci`.
Version changes stay in the existing Compose/default files. Each test run still
has an isolated writable daemon and chain state.

CI sets `LOCAL_ENV_IMAGE_PULL_POLICY=never` after preloading. Startup checks local
availability and fails if an image is absent; Compose also receives `--pull never`.
This avoids redundant registry requests for private images in a daemon that does
not inherit the host's GHCR credentials.

The default policy remains `always`: refresh mutable tags, then fall back to a
local copy if the pull fails. `missing` checks locally first and pulls absent
images. Invalid policies fail before Docker is contacted.

Run `npm test` and `npx tsc --noEmit` from `local-environment/` for regression checks.
The tests mock Docker; resolving the CI image map uses only `docker compose config`
and does not start containers. Validate the actual stack and test results in CI,
including the image-loading phase and the absence of startup registry re-pulls.
