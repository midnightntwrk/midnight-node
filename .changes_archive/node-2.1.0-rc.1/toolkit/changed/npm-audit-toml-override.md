#toolkit

# Override toml to 4.3.0 to clear the npm audit gate

`@effect/cli` pins `toml@^3.0.0`, which carries two high advisories
(GHSA-82x6-q7mm-w9cf uncontrolled recursion, GHSA-v5mp-jgw5-2x6j prototype
pollution), so `+audit-npm` failed on every PR. Both are patched in 4.2.0; npm
reported "no fix available" only because the caret range cannot reach it.

Same maintainer and repo, and `parse(input, options?)` is source-compatible.
Parsed tables now come back null-prototype - that *is* the pollution fix.

PR: https://github.com/midnightntwrk/midnight-node/pull/2139
