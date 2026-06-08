use sp_consensus_slots::{Slot, SlotDuration};
use sp_runtime::traits::Header as HeaderT;
use sp_timestamp::Timestamp;

/// Derives a reference timestamp from a block header.
///
/// Callers use this to obtain a chain-time anchor for a header without depending on
/// consensus-specific predigest parsing.
pub trait BlockReferenceTimestamp<Header>: Send + Sync {
	/// Returns the reference timestamp for `header`, if one can be derived.
	fn reference_timestamp(&self, header: &Header) -> Option<Timestamp>;
}

/// Reference timestamp derived from a block header slot and slot duration.
#[derive(Clone, Copy)]
pub struct SlotBasedBlockReferenceTimestamp<F> {
	slot_duration: SlotDuration,
	extract_slot: F,
}

impl<F> SlotBasedBlockReferenceTimestamp<F> {
	/// Creates a timestamp source using `extract_slot` to read the block slot from `header`.
	pub fn new(slot_duration: SlotDuration, extract_slot: F) -> Self {
		Self { slot_duration, extract_slot }
	}
}

impl<Header, F> BlockReferenceTimestamp<Header> for SlotBasedBlockReferenceTimestamp<F>
where
	Header: HeaderT,
	F: Fn(&Header) -> Option<Slot> + Send + Sync,
{
	fn reference_timestamp(&self, header: &Header) -> Option<Timestamp> {
		(self.extract_slot)(header).and_then(|slot| slot.timestamp(self.slot_duration))
	}
}
