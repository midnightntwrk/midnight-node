#toolkit
# Only correct docker-mounted volume ownership when `RESTORE_OWNER` is set

The toolkit's docker image must chown mounted volumes so that its nonroot user has permission to access them. When the `RESTORE_OWNER` environment variable is set, the container will restore ownership of mounted volumes to that owner. When not set, we were restoring to `"1000:1000"`, but that user doesn't always exist; instead, we now only restore when asked.

PR: https://github.com/midnightntwrk/midnight-node/pull/144