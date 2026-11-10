#toolkit
# Replay chains running runtime 2.1.100

The toolkit aborted with `UnsupportedBlockVersion(2001100)` on any chain running the
runtime shipped in node 2.1.101. It now decodes those blocks with the 2.1.100 metadata.

PR: https://github.com/midnightntwrk/midnight-node/pull/2400
Issue: https://github.com/midnightntwrk/midnight-node/issues/2399
