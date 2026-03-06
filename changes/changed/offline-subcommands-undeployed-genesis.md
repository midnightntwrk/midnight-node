#node

# Unify genesis state source for offline subcommands

Offline subcommands (CheckBlock, ExportBlocks, ExportState, ImportBlocks, Revert, benchmarks) now derive genesis state from the chain specification instead of the hardcoded UndeployedNetwork default. Addresses Least Authority audit Issue L.

PR: https://github.com/midnightntwrk/midnight-node/pull/768
Ticket: https://shielded.atlassian.net/browse/PM-20203
