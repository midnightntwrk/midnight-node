#node #genesis
# Make toolkit-js optional in toolkit-image for genesis generation

When `GENERATE_TEST_TXS=false`, the toolkit-image no longer requires `toolkit-js-prep` to be built. This allows genesis generation to proceed without needing GITHUB_TOKEN when test transactions are not being generated, since `fetch-compactc` (which downloads from a private GitHub repository) is only needed for test contract compilation.

Added `INCLUDE_TOOLKIT_JS` argument to `toolkit-image` target that defaults to `true` but can be set to `false` to skip including toolkit-js artifacts.

PR: https://github.com/midnightntwrk/midnight-node/pull/676
JIRA: https://shielded.atlassian.net/browse/PM-21902
