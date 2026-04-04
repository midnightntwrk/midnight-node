#node #security
# Replace genesis state decode panics with error propagation

The node panicked on startup when the chain spec contained a missing, non-string,
or invalid-hex `genesis_state` property. This replaces three `unwrap()` calls in
`run_node` with a `decode_genesis_state` helper that returns descriptive errors
via `sc_cli::Error::Input`, matching the existing error handling pattern used for
seed file loading. Adds a 256 MiB size guard against adversarial chain specs.

PR: https://github.com/midnightntwrk/midnight-node/pull/766
Ticket: https://shielded.atlassian.net/browse/PM-20204
