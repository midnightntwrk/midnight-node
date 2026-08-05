#node #local-env
# Let downstream repos run against a local-environment fork

Downstream repos (first up: midnight-indexer) can now point their services at a
`local-environment` fork via a stable docker network name and a per-run
connection manifest, with no coupling to this repo's layout.

PR: https://github.com/midnightntwrk/midnight-node/pull/1920
