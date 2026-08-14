//! Implements storage migrations of `pallet-session-validator-management`.
//!
//! V2 adds [`crate::QueuedCommittee`]. Stock [pallet_session] applies a validator set one session
//! after rotation, so the pallet tracks the queued committee separately from the active
//! [`crate::CurrentCommittee`].
//!
//! The v1-to-v2 cutover must use the combined
//! [`authority_keys::V1ToV2Migration`] when the runtime also changes its `AuthorityKeys` shape.
//! It translates `CurrentCommittee` and `NextCommittee`, seeds `QueuedCommittee` from the
//! translated current committee, and upgrades [pallet_session] key storage in one versioned step.
//! V1 has no `QueuedCommittee` storage.
//!
//! Do not run a separate queue-seeding migration before the key translation: it decodes old
//! committee bytes with current runtime types. Do not allocate another pallet storage version for
//! a runtime-local key change. See the migration module for runtime wiring and removal guidance.

pub mod authority_keys;
#[cfg(test)]
mod authority_keys_tests;
