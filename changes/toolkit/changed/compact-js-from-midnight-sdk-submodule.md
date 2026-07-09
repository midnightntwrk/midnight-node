#toolkit #dependencies
# Build compact-js/platform-js from the pinned midnight-sdk submodule

`util/toolkit-js/compact-0.31` now consumes `@midnight-ntwrk/{compact-js,
compact-js-node,compact-js-command,platform-js}` as `file:` tarballs built
from the new `midnight-sdk/` git submodule (pinned to the `cjs-2.5.1` release
tag) instead of npm registry tarballs. The submodule commit is the version of
truth; the `+midnight-sdk-js` Earthly target (`+midnight-sdk-js-local` on the
host, consumed by `+toolkit-js-prep` in CI) runs
`scripts/build-midnight-sdk-js.sh`, whose output is byte-reproducible so
`package-lock.json` integrity hashes stay stable. The older `compact-0.29`
and `compact-0.30` compatibility variants keep their registry pins.

PR: <link to PR>
