//! Implements storage migrations of `pallet-session-validator-management`.
//!
//! **Important:** It is crucial to run the migrations when upgrading runtime
//! to a version containing this pallet's storage version. Failing to do so
//! WILL break the chain and require a wipe or a rollback.
//!
//! To schedule a migration, add it to the `Executive` definition for your
//! runtime like this:
//!
//! ```rust, ignore
//! pub type Migrations = (
//! 	pallet_session_validator_management::migrations::v2::V1ToV2Migration<Runtime>,
//!     // ...
//! );
//! /// Executive: handles dispatch to the various modules.
//! pub type Executive = frame_executive::Executive<
//! 	Runtime,
//! 	Block,
//! 	ChainContext,
//! 	Runtime,
//! 	AllPalletsWithSystem,
//! 	Migrations,
//! >;
//! ```
//!
//! Each migration will be run only once, for the storage version for which it is
//! defined, and will update the storage version number.
//!
//! ## V2
//!
//! ### Changes
//!
//! This version adds the `QueuedCommittee` storage. Stock `pallet_session` applies a validator
//! set provided at rotation only one session later, so a rotation now moves `NextCommittee`
//! (selected) to `QueuedCommittee` (queued in `pallet_session`) and promotes the previously
//! queued committee to `CurrentCommittee`. `CurrentCommittee` thereby keeps its original
//! meaning: the committee whose keys form the effective validator set of the current session.
//!
//! ### Migration from V1
//!
//! Migration logic is provided by the `migrations::v2::V1ToV2Migration` migration. It
//! initializes `QueuedCommittee` with the value of `CurrentCommittee`, which under the v1
//! session integration was both the active and the queued validator set.
//!
//! ## Changing `AuthorityKeys`
//!
//! When `T::AuthorityKeys` changes shape, use `migrations::authority_keys::AuthorityKeysMigration`
//! wired into `SingleBlockMigrations` with `FROM`/`TO` set to the pallet's on-chain storage
//! versions before/after the upgrade. See that module's docs. This migration upgrades
//! `CurrentCommittee`, `QueuedCommittee`, and `NextCommittee`, plus `pallet_session` key storage.

pub mod v2;

pub mod authority_keys;
#[cfg(test)]
mod authority_keys_tests;
