# Enable ARM64 builds in CI pipelines

ARM64 Docker image builds are now available in PR and merge queue pipelines.
For PRs, add the `ci:arm64` label to trigger ARM64 builds. Merge queue and
manual workflow dispatches always run ARM64 builds.

PR: https://github.com/midnightntwrk/midnight-node/pull/375
Ticket: https://shielded.atlassian.net/browse/PM-21000

