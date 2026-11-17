#node #toolkit
# Run partner-chains wizard commands without a shell

The `wizards` subcommands built each external command as a single string and ran it with `sh -c`,
so config values such as base paths, bootnodes and the DB-Sync connection string were subject to
shell parsing.

- `IOContext::run_command` takes the program and its arguments separately and spawns the process
  directly. `start-node`, `generate-keys` and `register3` use it.
- `start-node` sets the node's environment variables on the wizard process instead of prefixing
  them to the command.
- `start-node` rejects a chain config whose bootnodes are not multiaddresses ending in
  `/p2p/<peer id>`.

Paths entered in the wizards are no longer shell-expanded, so `~` and `$VAR` are taken literally.

PR: https://github.com/midnightntwrk/midnight-node/pull/2430
