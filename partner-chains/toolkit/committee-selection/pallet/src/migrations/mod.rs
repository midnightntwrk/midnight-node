//! Implements storage migrations of `pallet-session-validator-management`.
//!
//! V2 adds [`crate::QueuedCommittee`]. Stock [pallet_session] applies a validator set one session
//! after rotation, so the pallet tracks the queued committee separately from the active
//! [`crate::CurrentCommittee`].
//!
//! [`v2::V1ToV2Migration`] performs the v1-to-v2 cutover; see its module docs for wiring.

pub mod v2;
#[cfg(test)]
mod v2_tests;
