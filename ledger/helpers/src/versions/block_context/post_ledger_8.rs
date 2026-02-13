pub fn make_block_context(
	tblock: super::base_crypto::time::Timestamp,
	parent_block_hash: super::base_crypto::hash::HashOutput,
	last_block_time: super::base_crypto::time::Timestamp,
) -> super::onchain_runtime::context::BlockContext {
	super::onchain_runtime::context::BlockContext {
		tblock,
		tblock_err: 30,
		parent_block_hash,
		last_block_time,
	}
}
