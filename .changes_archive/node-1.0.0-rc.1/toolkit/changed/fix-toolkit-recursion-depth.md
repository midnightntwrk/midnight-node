#toolkit
# Fix hitting recursion depth on context fork

Toolkit now uses `get_lazy` rather than `get` to avoid loading the entire ledger state when forking the context.

PR: https://github.com/midnightntwrk/midnight-node/pull/881
Ticket: https://shielded.atlassian.net/browse/PM-22253
