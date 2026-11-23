#local-env #fork-testing
# Snapshot forks: accept a local archive in `--from-snapshot`

`--from-snapshot` now also takes a local archive path or `file://` URL, so a large snapshot (preview: 12.7 GB) is downloaded once and reused across forks. Staging moved from the system temp dir into the network's `data/` dir, so `TMPDIR` no longer needs to point at a large disk.

PR: https://github.com/midnightntwrk/midnight-node/pull/2313
