#toolkit

# Add welcome contract e2e test

Ports the welcome contract from midnight-contracts and drives
deploy -> add_participant -> check_in through the full
compile/prove/submit/on-chain-verify pipeline. Adds a config template and a
`generate_intent_deploy_with_args` helper for passing constructor arguments.

Also fixes `prerequisites_ready` to match the compact variant directory by
major.minor.patch (previously major.minor), without which all contract e2e
tests silently skip. When `compact-contract-tests` is enabled, missing
prerequisites now fail the test instead of reporting a successful skip.

The welcome constructor is simplified from the upstream
`Vector<5000, Maybe<Opaque<"string">>>` to a small fixed vector of plain
strings, which the toolkit's CLI arguments can express.

PR: https://github.com/midnightntwrk/midnight-node/pull/1940
