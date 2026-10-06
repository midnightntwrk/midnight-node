#toolkit #bugfix
# Fix toolkit-js stack overflow on Node.js 24.21+

From Node 24.21 `require.resolve` is routed through `module.registerHooks`,
so toolkit-js's compact-js resolve hook re-entered itself until the stack
overflowed (`RangeError: Maximum call stack size exceeded`). The hook now
skips interception for a specifier it is already resolving, as on main.

PR: https://github.com/midnightntwrk/midnight-node/pull/2179
