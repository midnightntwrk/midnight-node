#toolkit
# Fix Dust address format to match the specification

- Corrected the prefix from `dust-addr` to `dust` (source: https://github.com/midnightntwrk/midnight-architecture/pull/190)
- Use `untagged_serialization` (source: https://github.com/midnightntwrk/midnight-architecture/blob/main/components/WalletEngine/Specification.md#dust-address)

PR: https://github.com/midnightntwrk/midnight-node/pull/1059
Ticket: https://shielded.atlassian.net/browse/PM-22375
