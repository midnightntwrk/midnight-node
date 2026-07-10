#toolkit
# Add regression tests for nonce/nullifier distinction in zswap serialization

Add unit tests verifying that serialized zswap local state uses the coin
nonce (randomness), not the nullifier (spend identifier), for the nonce
field. Addresses Least Authority Q1 2026 Node DIFF audit Issue E.

PR: https://github.com/midnightntwrk/midnight-node/pull/1128
JIRA: https://shielded.atlassian.net/browse/PM-22025
