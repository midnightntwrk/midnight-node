# CI local-environment image loading

`+local-env-ci` starts a fresh nested Docker daemon for each stack. The host's
Docker image store is separate from that daemon. The five private images and
five public dependencies (Cardano node, db-sync, Kupo, Ogmios and PostgreSQL) are
loaded through `WITH DOCKER --pull`, which reuses the shared BuildKit image cache
before transferring layers into the nested daemon.

The references are resolved with Docker Compose from the same defaults and
private-image overrides used to start the `withindexer` stack.
`local-environment/src/lib/ciImages.ts` checks that every active image-backed
service is accounted for. Build-only services are built by Compose; their
Dockerfile base images are not part of the service image preload list.

The existing digest-pinned BusyBox, socat, indexer PostgreSQL and NATS references
remain on Docker's verified pull path. EarthBuild v0.8.18 cannot retag digest
references through its embedded registry. Do not strip those pins to make loading
succeed. A digest-aware EarthBuild release is needed before moving these images
into the preload list:
https://github.com/EarthBuild/earthbuild/pull/513

When adding an image-backed CI dependency, update the mapping in `ciImages.ts`
and its corresponding loading strategy in `+local-env-ci`. Version changes stay
in the existing Compose/default files. Each run still has an isolated writable
daemon and chain state.

CI sets `LOCAL_ENV_IMAGE_PULL_POLICY=missing`. Startup uses existing tags without
registry requests and pulls only missing references, preserving digest validation.
Compose also receives `--pull missing`. This avoids redundant requests for
preloaded private images in a daemon without the host's GHCR credentials.

The default policy remains `always`: refresh mutable tags, then fall back to a
local copy if the pull fails. `never` is available for stacks where every exact
reference is preloaded. Invalid policies fail before Docker is contacted.

Run `npm test` and `npx tsc --noEmit` from `local-environment/` for regression checks.
The tests mock Docker; resolving the CI image map uses only `docker compose config`
and does not start containers. Validate the actual stack and test results in CI,
including cached public-image loading and the absence of private-tag re-pulls.
