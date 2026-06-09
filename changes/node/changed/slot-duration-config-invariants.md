#audit #node #hardening
# Reject incoherent mainchain timing configuration

Midnight nodes derive consensus-critical slot and epoch parameters from local
mainchain timing configuration that was previously mapped into the consensus
path without any coherence check, so an internally-incoherent or out-of-step
configuration could silently become a consensus input and split the network's
view of which blocks are valid.

This adds Layer-1 invariant validation. Four self-contained invariants on the
mainchain timing values (epoch duration non-zero, slot duration non-zero,
epoch duration at least one second, epoch duration divisible by slot duration)
are enforced at configuration-parse time via the existing `serde_valid` seam on
`MidnightCfg`, so an incoherent config is rejected as a config error before
service construction. A fifth, cross-field check (mainchain config vs. the
sidechain slot config) is enforced at the single `CreateInherentDataConfig`
aggregation choke point by making the constructor fallible and propagating the
error at both service construction sites. The long-standing `// TODO ETCM-4079`
divisibility acknowledgement at the construction site is resolved.

Scoped to Layer 1 only. The cross-node mechanisms a prior draft (PR #764)
attempted — an on-chain canonical source with a startup consistency check, and
a per-block configuration-hash digest committed in block headers — are deferred
out of scope, so audit Issue J is advanced by this change, not fully closed.

Issue: https://github.com/midnightntwrk/midnight-security/issues/78
JIRA: https://shielded.atlassian.net/browse/PM-20013
PR: https://github.com/midnightntwrk/midnight-node/pull/1656
