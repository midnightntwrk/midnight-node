#runtime #session-keys

# `generate_session_keys` no longer generates a `crch` key

The `SessionKeys_generate_session_keys` runtime API generated a new cross-chain (`crch`)
key into the keystore alongside the session keys, although the `crch` key identifies the
Cardano registration and is never rotated. It now generates the session keys only.

PR: https://github.com/midnightntwrk/midnight-node/pull/2488
