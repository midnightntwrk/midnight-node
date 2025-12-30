#client #runtime
# Fix permissioned nodes validity

Ensure that all keys are included in the runtime api response, so that the RPC can check them against expected values, and assigned a correct isValid value.

PR: https://github.com/midnightntwrk/midnight-node/pull/284
Ticket: https://shielded.atlassian.net/browse/PM-20398
