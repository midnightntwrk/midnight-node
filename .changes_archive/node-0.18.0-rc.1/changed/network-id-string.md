#runtime
# Change `NetworkId` type to `String`

Removes the `NetworkId` enum in favour of using an arbitrary `String`.

When generating chainspecs, we re-use the chainspec `id` field for the `network_id`.

PR: https://github.com/midnightntwrk/midnight-node/pull/171
Ticket: https://shielded.atlassian.net/browse/PM-19916
