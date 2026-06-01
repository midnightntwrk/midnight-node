#toolkit
# Run the `toolkit-js` test suite against every supported `compactc` version

The `Deploy.test.ts` test was skipped because importing
`@midnight-ntwrk/compact-js-command` directly picked up whichever
`compact-js` line npm happened to hoist, rather than the variant pinned for the
active `compactc` version.

Extracted the module-resolution hook from `bin.ts` into a shared
`compactc-resolver` module so the CLI and tests dispatch identically, and added
a vitest setup file that installs it for the active `COMPACTC_VERSION`. The test
now resolves `compact-js*` / `compact-runtime` (including the transitive imports
reached while loading a contract config) against the correct variant, so it
exercises the same behaviour as production.

Because the generated `managed/` output bakes in an expected `compact-runtime`,
each version must be tested in its own process with the contract recompiled.
Added `scripts/test-all-compactc.sh` (`npm run test:compat`) which recompiles the
test contract and runs the suite once per supported version.

PR:
