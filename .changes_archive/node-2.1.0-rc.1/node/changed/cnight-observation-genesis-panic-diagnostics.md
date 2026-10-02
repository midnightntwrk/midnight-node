#audit #hardening
# Improve cnight-observation genesis panic diagnostics

The genesis-build path for the cnight-observation pallet now reports the
chain-spec field path, the supplied byte length, and the maximum permitted
length when a value exceeds its bounded-vector cap, replacing four short
"expected" panic strings with actionable diagnostics for operators reading
a startup-failure log. The field paths match the camelCase JSON keys the
operator edits in chain-spec files (e.g.
`cNightObservation.config.addresses.<field>`).

PR: https://github.com/midnightntwrk/midnight-node/pull/1466
Issue: https://github.com/shieldedtech/shielded-security-engineering/issues/365
JIRA: https://shielded.atlassian.net/browse/PM-19896
