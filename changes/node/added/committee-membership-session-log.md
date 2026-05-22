#node
# Log on each session change whether this validator is in the committee

Validators now emit one log line per substrate session indicating whether
the local AURA key matches a member of the active committee. INFO when the
node is in the committee (with the matching AURA key as hex); WARN when it
is not (with the local AURA keys as hex and the committee size). Only
validators run the watcher - non-validator nodes stay silent.

Motivated by an incident where a validator silently failed to produce
blocks because its keystore held the wrong AURA key — the standard logs
gave no indication.

PR: https://github.com/midnightntwrk/midnight-node/pull/1534
Issue: https://github.com/midnightntwrk/midnight-node/issues/1399
