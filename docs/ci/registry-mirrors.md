# Docker Hub mirrors in hosted CI

On GitHub-hosted runners, `.envrc` selects `.earthly/config.hosted.yml`, which
sends `docker.io` pulls through `mirror.gcr.io`.
BuildKit falls back to Docker Hub when an image is absent from the cache.
The mirror is a cache of public Hub images, not a mirror of GHCR or a guarantee
that any particular image remains cached.

Hosted reusable E2E jobs also configure the host Docker daemon with an inline
Bash step. It preserves unrelated `daemon.json` keys, sets the GCR mirror and
reloads Docker without restarting it. The step is best effort: setup failure
does not fail the E2E job, and pulls continue using Docker's active configuration.

Self-hosted E2E jobs skip this step. Runner host Docker mirrors are managed
through `shielded-iac`; both the local/default and self-hosted configs are
unchanged. Unknown runner environments retain the default config. Changing
shared BuildKit settings requires coordinating all clients because its settings
hash can cause the CLI to replace the daemon during other jobs. Host Docker
mirrors do not automatically configure either BuildKit or nested Docker stores.

Rollback removes the mirror step and additional BuildKit configuration; no
shared self-hosted daemon setting changes are needed.

References:

- https://docs.docker.com/reference/cli/dockerd/#configuration-reload-behavior
- https://docs.docker.com/build/buildkit/toml-configuration/
- https://docs.cloud.google.com/artifact-registry/docs/pull-cached-dockerhub-images
