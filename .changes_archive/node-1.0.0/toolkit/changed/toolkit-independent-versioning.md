#toolkit
# Toolkit images are now versioned independently

The midnight-node-toolkit Docker image is now versioned from its own
`util/toolkit/Cargo.toml` instead of sharing the node version from
`node/Cargo.toml`. Release tags for toolkit-only releases use the
`toolkit-X.Y.Z` format.

PR: https://github.com/midnightntwrk/midnight-node/pull/1261
