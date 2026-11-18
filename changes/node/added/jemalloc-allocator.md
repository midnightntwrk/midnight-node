#node #metrics

# Use jemalloc as the global allocator on Linux, with allocator metrics and opt-in heap profiling

On Linux the node binary now uses jemalloc (`tikv-jemallocator`) as its global allocator, as
polkadot does.

jemalloc's memory statistics are published to Prometheus every 5 seconds as
`midnight_memory_allocator_bytes{stat=...}`, with one series each for `allocated`, `active`,
`metadata`, `resident`, `mapped` and `retained`.

Building with the new `jemalloc-profiling` cargo feature compiles in jemalloc's heap profiler.
It stays off until it is enabled with `MALLOC_CONF` at run time. See
`docs/operations/heap-profiling.md` for how to run the node with profiling and read the dumps.

PR: https://github.com/midnightntwrk/midnight-node/pull/2373
Issue:
