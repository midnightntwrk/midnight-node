#node #security

# Sandbox the node with Landlock on Linux

On Linux the node now restricts itself with Landlock in two stages.

Before it starts any threads, it confines writes to the base path, the keystore path, the
directory of `--node-key-file` and the temp dir, and blocks exec entirely. Reads stay open while
start-up loads its config, chain spec and seed files.

Once the service is up, on kernels with Landlock ABI v8 it also confines reads, on every thread,
to those directories plus `/etc`, `/usr`, `/lib*`, `/proc`, `/sys`, `/dev`, the node binary and
`ssl_root_cert`. Older kernels log a warning and keep reads open. Kernels without Landlock (before
5.13) log a warning and run unsandboxed.

Set `LANDLOCK=false` (cfg key `landlock`) to turn it off, for example to write jemalloc heap
profiles outside the base path.

PR: https://github.com/midnightntwrk/midnight-node/pull/2381
Issue: https://github.com/midnightntwrk/midnight-node/issues/2380
