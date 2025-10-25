# Removed Mapping and replaced with MappingEntry

`MappingEntry` is a stricter version of the `Mapping` type. Instead of having two different types that represent the same thing, we now just use `MappingEntry` everywhere that `Mapping` was used.

Ticket: https://shielded.atlassian.net/browse/PM-19897
PR: https://github.com/midnightntwrk/midnight-node/pull/166
