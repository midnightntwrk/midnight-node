# Heap profiling

On Linux the node uses jemalloc as its global allocator. This guide covers the two ways to look
into its memory use: the allocator metrics that every build exports, and heap profiles from a
build with the `jemalloc-profiling` feature.

## Allocator metrics

Every Linux build exports `midnight_memory_allocator_bytes` on the Prometheus endpoint. It is
refreshed every 5 seconds and has one series per jemalloc statistic (`stat` label):

| `stat`      | Meaning                                                                    |
|-------------|----------------------------------------------------------------------------|
| `allocated` | Bytes in live allocations requested by the node.                           |
| `active`    | Bytes in pages that hold live allocations (`allocated` plus page slack).   |
| `metadata`  | Bytes jemalloc uses for its own bookkeeping.                               |
| `resident`  | Bytes jemalloc holds in physical memory. The allocator's share of RSS.     |
| `mapped`    | Bytes in chunks jemalloc has mapped from the OS.                           |
| `retained`  | Bytes of virtual memory jemalloc kept after returning the pages to the OS. |

How to read them when memory grows:

- `allocated` grows: the node holds more and more live data, either from a leak or from a
  cache with no bound. A heap profile will show where it is allocated.
- `allocated` is flat and `resident` grows: the memory is fragmented, or jemalloc has not
  returned freed pages to the OS yet. A heap profile won't show anything, so look at
  allocator tuning instead.
- Process RSS grows but `resident` does not: the memory is outside jemalloc, for example
  ParityDB's memory-mapped files (`RssFile` in `/proc/<pid>/status`) or the wasm executor's
  mappings.

## Heap profiles

### Build

```bash
CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
  cargo build --release -p midnight-node --features jemalloc-profiling
```

The feature builds jemalloc with `--enable-prof`. Without debug line tables the profile shows
bare addresses. They don't slow the node down, but they make the binary much larger (about
1.2 GB).

Profiling is compiled in but off. A binary built with the feature and run without
`MALLOC_CONF` behaves like a normal build, apart from a little sampling bookkeeping.

### Run

Enable profiling through `MALLOC_CONF`. The node's jemalloc is unprefixed, so the variable is
`MALLOC_CONF`, not `_RJEM_MALLOC_CONF`.

```bash
mkdir -p /data/heap
MALLOC_CONF=prof:true,prof_final:true,lg_prof_interval:31,prof_prefix:/data/heap/node \
  ./target/release/midnight-node ...
```

- `prof:true` turns sampling on. Allocations are sampled about once every 512 KiB
  (`lg_prof_sample:19`, the default), which costs a few percent of throughput.
- `lg_prof_interval:31` writes a dump every 2^31 bytes (2 GiB) allocated in total. Lower it
  for more frequent dumps. Dumps are named `<prefix>.<pid>.<seq>.i<n>.heap`.
- `prof_final:true` writes a last dump when the process exits normally, which includes a
  graceful shutdown on `SIGTERM`. It doesn't write one when the process is killed with
  `SIGKILL` or by the OOM killer.
- `prof_prefix` must point at a writable directory. Without it, dumps go to the working
  directory as `jeprof.*.heap`. Dumps are small (tens to hundreds of KiB), but with a low
  `lg_prof_interval` there are a lot of them.
- The Landlock sandbox only lets the node write beneath its base path and the temp dir. Put
  `prof_prefix` under one of those, or set `LANDLOCK=false`.

### Analyse

The build compiles its own `jeprof`. It is a Perl script, found at:

```bash
find target/release/build -path '*tikv-jemalloc-sys*' -name jeprof -type f
```

The `jeprof` that ships with a distribution's jemalloc 5.x package (`libjemalloc-dev` on
Debian/Ubuntu) works as well.

The most useful view for growth is the difference between an early and a late dump. It shows
the call stacks whose live memory grew in between:

```bash
jeprof --text --cum --base=/data/heap/node.123.10.i10.heap \
  ./target/release/midnight-node /data/heap/node.123.80.i80.heap | head -40
```

Use `--cum`. In the default flat view almost all memory is attributed to jemalloc's own
`prof_backtrace_impl` frame, and the node's code only appears in the cumulative column.

To render the same comparison as a call graph:

```bash
jeprof --svg --base=early.heap ./target/release/midnight-node late.heap > growth.svg
```

Pass the binary that wrote the dumps. A binary from another build resolves the addresses
wrongly.
