#runtime #toolkit
# Remove pallet_sudo and add sudo-call toolkit command

Removes `pallet_sudo` from the runtime and adds a new `sudo-call` command to the toolkit that executes calls with Root origin through the federated authority governance mechanism (Council + Technical Committee approval).

PR: https://github.com/midnightntwrk/midnight-node/pull/455
Ticket: https://shielded.atlassian.net/browse/PM-9164
