#node #docker #security

# Remove gdb from the node and hardfork-test-upgrader images

Drop `gdb-16.3` from `images/node/Dockerfile` and
`images/hardfork-test-upgrader/Dockerfile`. A debugger shipped in the runtime
image is an attack-surface liability - it gives anyone with exec access
ptrace-based memory inspection of a running validator, including key
material - and it pulls in a sizeable dependency closure for something
nobody uses in production. `strace`, `procps-ng`, `vim`, `jq` and `tree`
remain for routine triage; on the rare occasion a core dump needs a
debugger, copy it out and open it off-host.

PR: https://github.com/midnightntwrk/midnight-node/pull/2187
