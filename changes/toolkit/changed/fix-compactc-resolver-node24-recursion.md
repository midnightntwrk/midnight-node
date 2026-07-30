#toolkit
# Fix toolkit-js compactc-resolver on Node 24+

Node 24 routes CJS `require.resolve` through `registerHooks`, so the
compactc-resolver's own resolve hook re-entered itself and recursed until the
stack overflowed, breaking every toolkit invocation.

- Resolver now tracks in-flight specifiers and defers to Node's default
  resolution on re-entry, breaking the recursion.
- No behavior change on Node 22 (the guard is a no-op there).

PR: https://github.com/midnightntwrk/midnight-node/pull/1711
