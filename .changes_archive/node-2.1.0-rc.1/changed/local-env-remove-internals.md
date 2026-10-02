# Remove internal Kubernetes and AWS coupling from local-environment

Cleans up the `local-environment/` tooling so it can be run without
Kubernetes or AWS access. Removes the `snapshot` command and its
supporting modules (`connectToPostgres`, `getSecretsForEnv`, `keystore`,
`portForwardWatchdog`, `previewProxy`, `snapshotEnv`), which fetched
secrets from cluster pods and uploaded archives to S3. Adds
`mockAuthorities` and `mockComposeOverride` so well-known networks can
be forked locally from a snapshot URI using mock validators instead.

PR: https://github.com/midnightntwrk/midnight-node/pull/1470
Issue: https://github.com/midnightntwrk/midnight-node/issues/1468
