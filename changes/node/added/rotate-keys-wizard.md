#node #toolkit #consensus

# rotate-keys wizard for generic session key generation and rotation

Added a `rotate-keys` wizard to the partner-chains toolkit CLI (`wizards rotate-keys`).
The wizard generates a fresh set of session keys on a running node via the
`author_rotateKeys` RPC call and decodes them with the runtime's own
`SessionKeys_decode_session_keys` API, so the key set is never hardcoded in the
toolkit — any key added to the runtime's `SessionKeys` (e.g. BABE during the
AURA to BABE migration) is picked up automatically and registered through the
generic-keys V1 registration datum.

The wizard then re-registers the candidate with the new keys, preserving the
cross-chain identity key. It supports two modes: fully automated (SPO cold
signing key and payment signing key available to the wizard) and staged, where
it prints a ready-to-run `register2` command for the cold machine, mirroring
the existing register1/2/3 split.

The wizard also supports an offline mode (`--runtime-wasm <path>`) that executes
the session-keys API of a provided runtime wasm blob directly (no node RPC
needed), writing the generated seeds into the node keystore. This allows
generating and registering session keys for a future runtime upgrade in
advance: the published wasm acts as the source of truth for the expected key
set (its blake2-256 hash is printed for verification against the announced
upgrade), and key types unknown to the provided runtime are carried over from
the existing keys file so the registration stays valid for the currently
active runtime as well.

This supersedes the approach proposed in the archived upstream PR
https://github.com/input-output-hk/partner-chains/pull/924.

PR:
