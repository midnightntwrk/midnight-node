# Docker Hub mirrors in hosted CI

On GitHub-hosted runners, `.envrc` selects `.earthly/config.hosted.yml`, which
sends `docker.io` pulls through `mirror.gcr.io`.
BuildKit falls back to Docker Hub when an image is absent from the cache.
The mirror is a cache of public Hub images, not a mirror of GHCR or a guarantee
that any particular image remains cached.

Hosted reusable E2E jobs also configure the host Docker daemon. A Bash composite action
preserves unrelated `daemon.json` keys and existing mirrors, validates a
temporary candidate with `dockerd --validate`, replaces it atomically, reloads
Docker with SIGHUP and checks the effective mirrors. It never restarts Docker.
Invalid configuration or unsupported reload fails before the file is replaced.

Self-hosted E2E jobs skip this helper. Runner host Docker mirrors are managed
through `shielded-iac`; both the local/default and self-hosted configs are
unchanged. Unknown runner environments retain the default config. Changing
shared BuildKit settings requires coordinating all clients because its settings
hash can cause the CLI to replace the daemon during other jobs. Host Docker
mirrors do not automatically configure either BuildKit or nested Docker stores.

`registry-mirror.yml` checks runner-profile selection with inline Bash, verifies
that the real Docker daemon and a running BusyBox keep their PIDs after reload,
and checks preservation of configuration, permissions and ownership. Reapplying
the action must leave the configuration inode unchanged. It then pulls a
distinct digest-pinned Alpine image and
then uses two separate small EarthBuild installations: one proves successful
Google mirror resolution; the other uses an unserved mirror path to force a
cache miss and prove fallback to Hub. Only allowlisted URL hosts/paths and
response statuses are emitted from the in-memory daemon debug logs.

The merge/reload action and validation use native Actions and inline Bash with
`jq`; there are no custom Python helpers. Real daemon and image-resolution probes
run only in hosted CI. Rollback removes the mirror step and additional BuildKit
configuration; no shared self-hosted daemon setting changes are needed.

References:

- https://docs.docker.com/reference/cli/dockerd/#configuration-reload-behavior
- https://docs.docker.com/build/buildkit/toml-configuration/
- https://docs.cloud.google.com/artifact-registry/docs/pull-cached-dockerhub-images
