#toolkit
# Update default CompactC version to 0.35.0

Bumped `COMPACTC_VERSION` from `0.33.0-rc.1` to `0.35.0` and added a `compact-0.35.0`
toolkit-js variant pinning `@midnight-ntwrk/compact-js*` `3.0.0-rc.3` and
`@midnight-ntwrk/compact-runtime` `0.20.0`, the runtime compactc 0.35.0 targets. The root `@midnightntwrk/ledger-v9` override moves
from `1.0.0-rc.3` to `1.0.0-rc.6` to satisfy compact-js 3.0.0-rc.3; it applies to every
variant, so `compact-0.33.0` now resolves rc.6 too.

compactc 0.35.0 appends a commit and date to `--version`, so the CI image, `.envrc` and
toolkit test version checks now compare only its first word.

