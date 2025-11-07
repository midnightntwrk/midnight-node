#cnight-generates-dust
# Add support for cNight genesis data in chain-spec

This is specified via a new config file, `cnight-genesis.json`. The config file can be generated using a new sub-command of the node, `generate-c-night-genesis`. It requires a db-sync connection to the target Cardano network.

PR: https://github.com/midnightntwrk/midnight-node/pull/160
Tickets:
- https://shielded.atlassian.net/browse/PM-19683
- https://shielded.atlassian.net/browse/PM-19682
- https://shielded.atlassian.net/browse/PM-19172
