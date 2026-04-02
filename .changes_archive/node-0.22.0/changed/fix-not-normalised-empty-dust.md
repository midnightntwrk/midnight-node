#toolkit
# Fix `NotNormalized` error when dust spends and registrations are empty

Fixes the toolkit to omit `DustActions` in a transaction if it's a No-op.
Resolves `NotNormalized` errors.

PR: https://github.com/midnightntwrk/midnight-node/pull/758
Ticket: https://shielded.atlassian.net/browse/PM-21958
