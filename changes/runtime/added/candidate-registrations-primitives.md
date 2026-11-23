#runtime #node #registrations

# Primitives for permissionless candidate registrations

Adds `midnight-primitives-candidate-registrations`, the types the node and the runtime
will share for permissionless stake pool ("NIGHT") candidate registrations, as the
NIGHT Staking on Cardano MIP specifies them:
<https://github.com/midnightntwrk/midnight-improvement-proposals/blob/ab25a20dc01cfab0ee39013b06234b3cc077d155/mips/mip-xxxx-night-staking-on-cardano.md>.
The MIP rule numbers in the crate's documentation refer to that revision.

The crate holds the registration as it crosses from Cardano to Midnight; the record the
runtime keeps per pool, with the past states committee selection reads (for Cardano epoch
`E`, the registrations at the start of `E - 3`, MIP rule 16); the messages a
registration's signatures cover (MIP rule 21), with golden tests of their encodings; the
inherent identifier and error; and the `CandidateRegistrationsApi` runtime API.

Nothing depends on the crate yet: it is compiled into no binary and changes no behaviour.

PR: https://github.com/midnightntwrk/midnight-node/pull/2474
Issue: https://github.com/midnightntwrk/midnight-node/issues/2495
