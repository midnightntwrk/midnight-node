#runtime #governance
# Improved Federated Authority Observation inherent

Improve the `pallet_federated_authority_observation` inherent mechanism by:

- Rejecting duplicated Midnight (sr25519) members. This was a security issue, as duplicated members cannot vote multiple times. Having too many duplicated members could potentially make it impossible to reach the required authority body motion thresholds. For example, with a Council of 6 members and a threshold of 2/3, if one member occupies 4 seats, it would be impossible to reach the required 4 votes.
- Fixing the `check_inherent` function to verify that the queried members match those set in the inherent `call`.

PR: https://github.com/midnightntwrk/midnight-node/pull/441
Ticket: https://shielded.atlassian.net/browse/PM-20680
