#runtime #node #babe

# Add the BABE key to session keys

`opaque::SessionKeys` gains a `babe` key next to `aura` and `grandpa`, so the committee's BABE
authorities are kept in `pallet-session` and handed to `pallet-babe` like any other session key.
Conversions in both directions (`from_candidate_keys`, `CandidateKeys`) carry it, so a
permissioned candidate registered on Cardano is expected to publish a `babe` key.

Because the key set changed, everything that produces or consumes authority keys was extended:

- `res/<network>/permissioned-candidates-config.json` (all networks) and the mock bridge data
  gain a `babe_pub_key` entry; `InitialAuthorityData` requires it, and genesis / chain-spec
  construction writes it into `SessionKeys`.
- `generate_permissioned_candidates_genesis` reads the `babe` key from the candidate keys and
  skips (with a warning) candidates that are missing AURA, BABE or GRANDPA.
- The partner-chains CLI knows a `BABE` key definition (`sr25519`, key type `babe`) and the
  Midnight runtime lists it in `key_definitions()`, so the key-generation and registration
  wizards handle it.

For the migration window the existing BABE keys are copies of the AURA keys (see the combined
committee/session-key migration), and `AuraToBabeMigrationKeystore` answers BABE queries from the
AURA key, so signatures stay valid for validators that have not yet added a BABE key. That
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
