#runtime #committee-selection

# Remove candidates with duplicated keys from the candidates list

Permissioned candidate that is further down the candidates list but that has written key already used by another candidate will be filtered out.
Such situation in permissioned candidates list should not happen. The list should be reviewed by technical committee before being written to Cardano.
This filtering happens after validating candidates.

From valid registered candidates list every candidate that has same key as another candidate is removed.
This happens pair-wise: both candidates are dropped, because we don't know which one was the rightful owner of the key.
Both proved that they have key (signatures are verified before), we can't tell which one stole the keys.
Also registered candidate is removed if his keys are present in permissioned candidates list.
Reasoning is that we can't have duplicates downstream and it is a choice of using permissioned over registered.

Note: permissionless registrations are not considered finished, we know that the process as in current sources does
not check for session keys ownership. There is a separate issue tracking it and will be fixed separately.

Issue: https://github.com/midnightntwrk/midnight-node/issues/2192
PR: https://github.com/midnightntwrk/midnight-node/pull/2193
