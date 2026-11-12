#ci #security
# Harden bot workflows against TOCTOU and expression injection
Fix compound TOCTOU vulnerability (M-F001) and expression injection findings (M-F002, M-F003, M-F004) in four comment-triggered bot workflows. Switch checkout from branch name to commit SHA, remove .envrc sourcing in favor of explicit EARTHLY_CONFIG, and migrate user-supplied inputs to env: block indirection.

PR: https://github.com/midnightntwrk/midnight-node/pull/848
Ticket: https://shielded.atlassian.net/browse/PM-22117
