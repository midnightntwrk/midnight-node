# Generating session keys

`midnight-node generate-session-keys` puts a fresh, complete set of session keys into
the node's keystore and prints their public keys for the Cardano registration. The key
set is whatever the runtime defines, so a runtime upgrade that adds a key type needs no
tool change. It is meant for rotating the session keys of a network that runs BABE;
the AURA to BABE migration does not use it and follows its own runbook.

## Running it

Run it on the validator's machine, as the user the node runs as, so the new key files
get the node's owner and permissions. The node can keep running: it sees the new keys
without a restart. In Docker:

```sh
docker exec -u <node-user> <container> \
  midnight-node generate-session-keys --keystore-path <node keystore directory> \
  --rpc-port <node RPC port>
```

`--keystore-path` is required: give the directory the running node uses as its
keystore, by default `<base-path>/chains/<chain id>/keystore`. If the keystore has a
password, pass it with `--password-filename` or `--password-interactive` (`--password`
would leave it in the process list and shell history). The tool reads no node
configuration.
Before writing, it prints the keystore's absolute path, with symbolic links resolved,
and its `crch` key to stderr, so you can check it is the node's keystore.

`--runtime-wasm <path>` generates the keys that runtime file defines, compressed
(`.compact.compressed.wasm`) or not, for example an upgrade that adds a key type and is
not on the chain yet. The tool then reads nothing from the chain and does not check the
file against it: make sure it is the runtime the network will run.

Without `--runtime-wasm` the node must be running: the tool reads the chain's current
runtime code over the node's RPC on `127.0.0.1`, at the port given with `--rpc-port`
(required in this case, there is no default, and rejected together with
`--runtime-wasm`). The tool runs whatever code the process listening on that port
serves, so make sure it is your node.

## What it checks

Before writing anything, the tool aborts if (the keystore is checked before the runtime
code is read):

- the keystore directory does not exist (it is never created);
- the keystore does not hold exactly one `crch` (cross-chain) key;
- the keystore cannot sign with that `crch` key, which means a wrong keystore password
  (see [Seed files](#seed-files) for a `crch` key from `CROSS_CHAIN_SEED_FILE`);
- the runtime does not run with this node's host functions;
- the runtime does not have `SessionKeysGenerationApi` version 1, as no runtime before
  this tool does.

The `crch` key is never rotated. Existing key files are never opened, except for the
`crch` key, which the keystore uses for that one test signature; the tool never sees its
secret and lists every other public key by file name only.

If something fails after keys were written, nothing is deleted and the error names
the new keystore files. This includes a runtime that writes a key it does not return as
a session key.

## Output

```json
{"sidechain_pub_key":"0x..","keys":{"aura":"0x..","gran":"0x..","babe":"0x..","beef":"0x.."},"signatures":{"aura":"0x..","gran":"0x..","babe":"0x..","beef":"0x.."},"runtime_code_hash":"0x.."}
```

`sidechain_pub_key` is the `crch` public key that identifies the registration.
`keys` and `signatures` list the keys in the runtime's key order. `signatures` holds each
key's proof of possession, as the runtime makes it: its signature over `b"POP_"` followed
by the SCALE-encoded `AccountId32` of the `crch` key, the statement `pallet_session` uses for
an ownership proof. Joined in that order they form the `pallet_session` ownership proof for
that account.
`runtime_code_hash` is the Blake2-256 hash of the runtime code the keys came from.

## After generating

Register the new public keys on Cardano. Keep the old keys in the keystore until the
new registration is active, two Cardano epochs later.

The new keys exist only in the keystore: the tool never prints their secrets. Keep the
keystore on persistent storage (by default it is under the base path, next to the chain
database); a keystore recreated on start loses them.

### Seed files

The node inserts the keys of `*_SEED_FILE` files into the keystore on every start.

- `CROSS_CHAIN_SEED_FILE` can stay: the `crch` key is never rotated.
- `AURA_SEED_FILE`, `BABE_SEED_FILE`, `GRANDPA_SEED_FILE` and `BEEFY_SEED_FILE` keep
  re-inserting the old key of their type. Remove those for the rotated keys once the new
  registration is active; removing the setting deletes no key file.
- The node derives seed-file keys without the keystore password. With a password-protected
  keystore, a `crch` key from a phrase seed cannot sign, so the tool reports a wrong
  password even when it is right; a `crch` key from a hex seed signs with any password,
  so a wrong password goes undetected.
