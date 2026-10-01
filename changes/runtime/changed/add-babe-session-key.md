#runtime #node #babe

# Add the BABE key to session keys

`opaque::SessionKeys` gains a `babe` key next to `aura` and `grandpa`, so the committee's BABE
authorities are kept in `pallet-session` and handed to `pallet-babe` like any other session key.
Conversions in both directions (`from_candidate_keys`, `CandidateKeys`) carry it, so a
permissioned candidate registered on Cardano is expected to publish a `babe` key.

Because the key set changed, everything that produces or consumes authority keys was extended:

- `InitialAuthorityData` requires a `babe_pub_key` entry, and genesis / chain-spec construction
  writes it into `SessionKeys`. The checked-in `permissioned-candidates-config.json` of `dev`,
  `devnet`, `govnet`, `guardnet`, `local`, `perfnet` and `stagenet` and the mock bridge data
  (`default-registrations.json`, `qanet-mock.json`) carry it. The `qanet`, `preview`, `preprod`
  and `mainnet` configs are untouched: they are regenerated from the Cardano candidate list, so
  their chain specs can only be rebuilt once every candidate has published a `babe` key.
- `generate_permissioned_candidates_genesis` reads the `babe` key from the candidate keys and
  skips (with a warning) candidates that are missing AURA, BABE or GRANDPA.
- The partner-chains CLI knows a `BABE` key definition (`sr25519`, key type `babe`) and the
  node's CLI bindings (`key_definitions()` in `node/src/cli.rs`) list it, so the key-generation
  and registration wizards handle it.

For the migration window the existing BABE keys are copies of the AURA keys (see the combined
committee/session-key migration), and the existing `AuraToBabeMigrationKeystore` answers BABE
queries from the AURA key, so signatures stay valid for validators that have not yet added a BABE key. That
fallback only covers the migration: from here on a validator is expected to hold a real BABE key
and to have it registered as the permissioned candidate's `babe` key on Cardano.

With the keys in place, `pallet-consensus-engine`'s `migrate_to_babe` no longer panics with
"Issue #1742 adds BABE keys to the runtime" — the flip now completes, bootstrapping
`pallet-babe`'s genesis slot, epoch index and randomness. Authorities are deliberately not written
there: `pallet-babe` is wired as a `pallet_session::OneSessionHandler`, so `Authorities` /
`NextAuthorities` already track the committee's BABE keys. Pallet tests assert the post-flip state
instead of the old panic.

Static runtime metadata is regenerated for the new `SessionKeys` shape.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
PR: https://github.com/midnightntwrk/midnight-node/pull/2113
