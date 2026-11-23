#node #landlock #cli

# Print the real error when the node stops after start-up

When the running node stopped on an error, it printed the config help before the error. Under the
Landlock read sandbox the help can no longer read `/res/cfg/default.toml`, so it panicked and the
`error:` line was never printed. The node now prints only the error on this path. Set
`SHOW_CONFIG=1` to see the config.

PR: TBD
Issue: https://github.com/midnightntwrk/midnight-node/issues/2506
