#audit #hardening
# Complete CIP-19 validation for Cardano reward addresses

Consolidate Cardano reward-address validation into a single validated
constructor `CardanoRewardAddressBytes::try_new(bytes, expected_network)` on
the cNIGHT observation primitive, and route every entry point through it so a
malformed, wrong-length, wrong-address-type, or wrong-network reward address is
rejected at the trust boundary with a specific, named error instead of being
stored and surfacing failures obscurely later. The constructor checks length
first, then the CIP-19 header's type nibble (14 or 15) and network nibble, using
`no_std` bit arithmetic; the length-only conversion is retained for test and
benchmark callers.

The chain follower's four reward-address construction sites (registration,
deregistration, asset-create owner, asset-spend owner) now share one helper
wrapping the validated constructor and skip a bad record gracefully rather than
panicking, the previously scattered ad-hoc parsing collapses to a single error
type, and the dead network-error variant is removed. Genesis build validates
deserialized reward-address keys against the address type and the network
derived from the mapping-validator address Bech32 prefix, failing fast on an
invalid key. The mapping-validator address, a different Cardano address
category, keeps its own structural validator returning a specific error and is
applied on both the genesis build and the `set_mapping_validator_contract_address`
extrinsic.

PR: https://github.com/midnightntwrk/midnight-node/pull/1817
Issue: https://github.com/shieldedtech/shielded-security-engineering/issues/498
JIRA: https://input-output.atlassian.net/browse/PM-20267
