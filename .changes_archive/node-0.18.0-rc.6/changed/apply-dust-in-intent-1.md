#toolkit
# Fix `register-dust-address` command when run on wallets with NIGHT

This command submitted a TX which was
 1. recreating all NIGHT UTxOs in the unshielded wallet in segment 1
 2. registering the DUST address in segment 2

The existing NIGHT UTxOs only start accumulating DUST when spent in the same segment as the registration. We now apply all DUST actions in segment 1.

PR: https://github.com/midnightntwrk/midnight-node/pull/140
JIRA: https://shielded.atlassian.net/browse/PM-20026