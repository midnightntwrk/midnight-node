#node #keystore #session-keys

# Add `generate-session-keys`

`midnight-node generate-session-keys --keystore-path <dir> (--runtime-wasm <path> | --rpc-port <port>)`
generates a fresh set of session keys, as the runtime defines it, into the given keystore
and prints their public keys and proofs of possession as JSON for the Cardano
registration. With `--runtime-wasm` it works offline on the given runtime file, compressed
or not; otherwise it reads the chain's current runtime from the node's RPC on 127.0.0.1.

Nothing is written unless the keystore directory exists, holds exactly one `crch` key and
can sign with it (a wrong password fails here), and the runtime runs with the node's host
functions and has `SessionKeysGenerationApi` version 1. See `docs/session-key-generation.md`.

PR: https://github.com/midnightntwrk/midnight-node/pull/2488
