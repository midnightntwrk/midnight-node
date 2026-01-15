#toolkit
# Add `deregister-dust-address` command to toolkit

Adds a new `deregister-dust-address` subcommand under `generate-txs` that allows
users to remove their DUST address mapping from the Midnight network.

This enables users to:
- Migrate to a new DUST address
- Clean up test registrations
- Revoke access before rotating wallet keys

PR: https://github.com/midnightntwrk/midnight-node/pull/482
Ticket: https://shielded.atlassian.net/browse/PM-20855
