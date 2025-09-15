pub mod time_units {
	use crate::BlockNumber;

	/// Milliseconds between Polkadot-like chain blocks.
	pub const MILLISECS_PER_BLOCK: u64 = 6000;

	/// A minute, expressed in Polkadot-like chain blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
	/// A hour, expressed in Polkadot-like chain blocks.
	pub const HOURS: BlockNumber = MINUTES * 60;
	/// A day, expressed in Polkadot-like chain blocks.
	pub const DAYS: BlockNumber = HOURS * 24;
}
