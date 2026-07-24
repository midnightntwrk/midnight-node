#runtime #consensus

# generate_session_keys no longer regenerates the cross-chain key

The `sp_session::SessionKeys::generate_session_keys` runtime API implementation
used to unconditionally generate a new cross-chain (`crch`) key in the node
keystore on every call and discard its public key. Since the cross-chain key is
the node's permanent partner-chain identity (the one the candidate is
registered with on Cardano), every `author_rotateKeys` call silently polluted
the keystore with unused identity keys.

The cross-chain key is now generated only if the keystore does not contain one
yet, so `author_rotateKeys` rotates consensus session keys without touching the
identity key. `impl_version` was bumped (0 -> 1); no storage or transaction
format changes.

PR:
