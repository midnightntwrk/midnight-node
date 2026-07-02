#toolkit
# Fix toolkit-js compactc-resolver infinite recursion on Node 24

The `COMPACTC_VERSION` resolution hook in `util/toolkit-js` (`compactc-resolver.ts`)
redirects `@midnight-ntwrk/compact-js*` / `compact-runtime` / `platform-js` imports to
the pinned variant workspace by calling `require.resolve` inside a `registerHooks`
resolve hook. Node 22 did not route CJS `require.resolve` through `registerHooks`, but
Node 24 does (`resolveForCJSWithHooks`), so the hook's own internal `require.resolve`
re-entered the hook for the same specifier and recursed until the stack overflowed
(`RangeError: Maximum call stack size exceeded`), breaking every toolkit invocation.

The resolver now tracks the specifiers it is actively resolving and, on re-entry, defers
to Node's default resolution (which is already rooted at the variant package), breaking
the recursion. The guard is a no-op on Node 22, so resolution is unchanged there.

PR: https://github.com/midnightntwrk/midnight-node/pull/1711
