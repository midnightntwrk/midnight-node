#toolkit
# Fix stack overflow in `trusted_deserialize_tagged` on long-running chains

`TrustedCacheLoader::get` recurses depth-first through the serialised
ledger arena: every call invokes `T::from_binary_repr(...)`, which
iterates the node's children and calls back into `Loader::get` for
each, producing one stack frame per arena node visited. On chains
long enough to produce deep storage trees (Cardano Preview after
~1 M blocks; ~3.7 M arena nodes in the reproducing run) the
recursion depth combined with per-frame size exceeded the default
2 MiB tokio worker thread stack and the process aborted with SIGABRT
mid-deserialise.

Wraps the body of `Loader::get` in `stacker::maybe_grow(64 KiB, 1
MiB, ...)`. When remaining stack drops below the red zone, `stacker`
allocates a fresh 1 MiB extension stack and resumes the recursion on
it; otherwise the call runs in place with essentially zero overhead.
Same pattern rustc, regex, syn, swc, and deno use for
deep-recursion-prone code. No API change, no caller responsibility,
no env var — the workaround of setting `RUST_MIN_STACK` per
invocation is no longer required.

Surfaced while validating PR #1574's cache-tag fix end-to-end via the
e2e suite's `dust_balance_smoke_many` (PR #1569). The bug was latent
until #1574 made snapshots tagged at the real chain head, exercising
the warm-cache restore path for the first time.

PR: https://github.com/midnightntwrk/midnight-node/pull/1576
Issue: https://github.com/midnightntwrk/midnight-node/issues/1575
