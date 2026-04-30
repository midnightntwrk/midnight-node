#node #runtime
# Replace panic with error handling in cnight-observation inherent data decoding

The `get_data_from_inherent_data` function in the `cnight-observation` pallet
used `.expect()` on inherent data decoding, which could cause all validators
to panic simultaneously on malformed data, halting the chain. This replaces
the panic with typed `Result<Option<...>, InherentError>` error handling
using a new `DecodeFailed` variant, matching the pattern already established
in the sibling `federated-authority-observation` pallet.

Fixes: https://github.com/midnightntwrk/midnight-node/issues/1317
PR: https://github.com/midnightntwrk/midnight-node/pull/1234
JIRA: https://shielded.atlassian.net/browse/PM-21799
