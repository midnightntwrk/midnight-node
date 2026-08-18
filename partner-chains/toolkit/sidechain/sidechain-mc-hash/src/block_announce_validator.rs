use crate::{
	BlockReferenceTimestamp, LocalDataUnavailableReason, McHashDataSource, McHashInherentDigest,
	StableBlockForHash,
};
use sidechain_domain::McBlockHash;
use sp_consensus::block_validation::{BlockAnnounceValidator, Validation};
use sp_partner_chains_consensus::InherentDigest;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::{error::Error, future::Future, pin::Pin, sync::Arc};

/// Block announcement validator that preflights the announced main-chain hash digest.
///
/// When the digest references a Cardano block that is stable locally, the announcement is
/// accepted. When the block is found but not yet stable or its timestamp is out of range,
/// the validator checks tip freshness: if our view is current the block is rejected as
/// invalid (without disconnect); if stale, the validation returns an internal error so the
/// SDK maps it to `Skip`, avoiding peer reputation penalties for a local db-sync lag.
/// When the hash is completely unknown, the validator uses Praos chain-density health
/// (`is_cardano_ok`) to disambiguate invalid references from local observability gaps.
#[derive(Clone)]
pub struct McHashBlockAnnounceValidator<RTS> {
	block_source: Arc<dyn McHashDataSource + Send + Sync>,
	reference_timestamp: RTS,
}

impl<RTS> McHashBlockAnnounceValidator<RTS> {
	/// Creates a validator using the provided main-chain hash data source.
	pub fn new(
		block_source: Arc<dyn McHashDataSource + Send + Sync>,
		reference_timestamp: RTS,
	) -> Self {
		Self { block_source, reference_timestamp }
	}
}

#[derive(Debug, thiserror::Error)]
enum McHashBlockAnnounceValidationError {
	#[error("local Cardano data unavailable while validating announced MC hash {hash}: {reason}")]
	LocalDataUnavailable { hash: McBlockHash, reason: LocalDataUnavailableReason },
	#[error(
		"failed to query local Cardano data while validating announced MC hash {hash}: {error}"
	)]
	DataSourceError { hash: McBlockHash, error: String },
	#[error("local Cardano view is not healthy while validating announced MC hash {0}")]
	CardanoNotOk(McBlockHash),
}

impl<B, RTS> BlockAnnounceValidator<B> for McHashBlockAnnounceValidator<RTS>
where
	B: BlockT,
	RTS: BlockReferenceTimestamp<B::Header>,
{
	fn validate(
		&mut self,
		header: &B::Header,
		data: &[u8],
	) -> Pin<Box<dyn Future<Output = Result<Validation, Box<dyn Error + Send>>> + Send>> {
		if !data.is_empty() {
			log::debug!("Received unknown data alongside the MC hash block announcement.",);
			return Box::pin(async { Ok(Validation::Failure { disconnect: true }) });
		}

		let mc_hash = match McHashInherentDigest::value_from_digest(header.digest().logs()) {
			Ok(mc_hash) => mc_hash,
			Err(err) => {
				log::debug!("Failed to retrieve MC hash from announced block digest: {err}");
				return Box::pin(async { Ok(Validation::Failure { disconnect: true }) });
			},
		};

		let timestamp = match self.reference_timestamp.reference_timestamp(header) {
			Some(timestamp) => timestamp,
			None => {
				log::debug!("Failed to derive reference timestamp from announced block header");
				return Box::pin(async { Ok(Validation::Failure { disconnect: true }) });
			},
		};

		let block_source = self.block_source.clone();

		Box::pin(async move {
			match block_source.get_stable_block_for(mc_hash.clone(), timestamp).await {
				Ok(StableBlockForHash::Found(_)) => Ok(Validation::Success { is_new_best: false }),
				Ok(StableBlockForHash::LocalDataUnavailable { reason }) => {
					Err(Box::new(McHashBlockAnnounceValidationError::LocalDataUnavailable {
						hash: mc_hash,
						reason,
					}) as Box<dyn Error + Send>)
				},
				Ok(
					StableBlockForHash::BlockFoundButNotStable { .. }
					| StableBlockForHash::BlockTimestampOutOfRange { .. },
				) => {
					let is_cardano_tip_fresh =
						block_source.is_cardano_tip_fresh().await.map_err(|error| {
							Box::new(McHashBlockAnnounceValidationError::DataSourceError {
								hash: mc_hash.clone(),
								error: error.to_string(),
							}) as Box<dyn Error + Send>
						})?;
					if is_cardano_tip_fresh {
						Ok(Validation::Failure { disconnect: false })
					} else {
						Err(Box::new(McHashBlockAnnounceValidationError::CardanoNotOk(mc_hash))
							as Box<dyn Error + Send>)
					}
				},
				Ok(StableBlockForHash::BlockNotFound { .. }) => {
					let is_cardano_ok = block_source.is_cardano_ok().await.map_err(|error| {
						Box::new(McHashBlockAnnounceValidationError::DataSourceError {
							hash: mc_hash.clone(),
							error: error.to_string(),
						}) as Box<dyn Error + Send>
					})?;
					if is_cardano_ok {
						Ok(Validation::Failure { disconnect: false })
					} else {
						Err(Box::new(McHashBlockAnnounceValidationError::CardanoNotOk(mc_hash))
							as Box<dyn Error + Send>)
					}
				},
				Err(error) => Err(Box::new(McHashBlockAnnounceValidationError::DataSourceError {
					hash: mc_hash,
					error: error.to_string(),
				}) as Box<dyn Error + Send>),
			}
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		BlockByHash, LatestStableBlockForTimestamp, MC_HASH_DIGEST_ID, MainchainBlock,
		SlotBasedBlockReferenceTimestamp, StableBlockForHash,
	};
	use sidechain_domain::{McBlockNumber, McEpochNumber, McSlotNumber};
	use sp_consensus_slots::{Slot, SlotDuration};
	use sp_runtime::DigestItem;
	use sp_runtime::OpaqueExtrinsic;
	use sp_runtime::testing::{Digest, Header};
	use sp_timestamp::Timestamp;
	use std::io;

	type Block = sp_runtime::generic::Block<Header, OpaqueExtrinsic>;

	const TEST_SLOT_DURATION: SlotDuration = SlotDuration::from_millis(6000);
	const TEST_SLOT: u64 = 100;
	const TEST_CONSENSUS_ENGINE_ID: [u8; 4] = *b"aura";

	#[derive(Clone, Copy)]
	enum Query<T> {
		Value(T),
		Error(&'static str),
		Panic(&'static str),
	}

	#[derive(Clone, Copy)]
	enum StableLookup {
		Found,
		LocalDataUnavailable,
		NotStable,
		TimestampOutOfRange,
		NotFound,
		Error,
		Panic,
	}

	struct TestDataSource {
		tip_fresh: Query<bool>,
		stable_lookup: StableLookup,
		cardano_ok: Query<bool>,
	}

	impl Default for TestDataSource {
		fn default() -> Self {
			Self {
				tip_fresh: Query::Value(true),
				stable_lookup: StableLookup::Found,
				cardano_ok: Query::Value(true),
			}
		}
	}

	impl TestDataSource {
		fn no_queries_expected() -> Self {
			Self {
				tip_fresh: Query::Panic("validator should not query tip freshness"),
				stable_lookup: StableLookup::Panic,
				cardano_ok: Query::Panic("validator should not query Cardano health"),
			}
		}
	}

	#[async_trait::async_trait]
	impl McHashDataSource for TestDataSource {
		async fn get_latest_stable_block_for(
			&self,
			_reference_timestamp: Timestamp,
		) -> Result<LatestStableBlockForTimestamp, Box<dyn Error + Send + Sync>> {
			unreachable!("block announce validator does not query latest stable block")
		}

		async fn get_stable_block_for(
			&self,
			hash: McBlockHash,
			_reference_timestamp: Timestamp,
		) -> Result<StableBlockForHash, Box<dyn Error + Send + Sync>> {
			match self.stable_lookup {
				StableLookup::Found => Ok(StableBlockForHash::Found(mainchain_block(hash))),
				StableLookup::LocalDataUnavailable => {
					Ok(StableBlockForHash::LocalDataUnavailable {
						reason: LocalDataUnavailableReason::LatestBlockUnavailable,
					})
				},
				StableLookup::NotStable => Ok(StableBlockForHash::BlockFoundButNotStable {
					hash,
					block_number: McBlockNumber(10),
					latest_block_number: McBlockNumber(15),
					required_latest_block: McBlockNumber(20),
				}),
				StableLookup::TimestampOutOfRange => {
					Ok(StableBlockForHash::BlockTimestampOutOfRange {
						hash,
						block_timestamp: Timestamp::new(1000),
						min_allowed_timestamp: Timestamp::new(2000),
						max_allowed_timestamp: Timestamp::new(3000),
						reference_timestamp: Timestamp::new(4000),
					})
				},
				StableLookup::NotFound => Ok(StableBlockForHash::BlockNotFound { hash }),
				StableLookup::Error => {
					Err(Box::new(io::Error::other("stable block lookup failed")))
				},
				StableLookup::Panic => unreachable!("validator should not query stable block"),
			}
		}

		async fn get_block_by_hash(
			&self,
			_hash: McBlockHash,
		) -> Result<BlockByHash, Box<dyn Error + Send + Sync>> {
			unreachable!("block announce validator does not query block by hash")
		}

		async fn is_cardano_tip_fresh(&self) -> Result<bool, Box<dyn Error + Send + Sync>> {
			query_bool(self.tip_fresh)
		}

		async fn is_cardano_ok(&self) -> Result<bool, Box<dyn Error + Send + Sync>> {
			query_bool(self.cardano_ok)
		}
	}

	#[tokio::test]
	async fn accepts_announcement_when_mc_hash_is_stable() {
		let mut validator = validator(TestDataSource::default());

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[])
			.await
			.expect("stable MC hash should be valid");

		assert_eq!(result, Validation::Success { is_new_best: false });
	}

	#[tokio::test]
	async fn accepts_stable_announcement_even_if_tip_is_not_fresh() {
		let mut validator = validator(TestDataSource {
			tip_fresh: Query::Panic("stable hash should not query tip freshness"),
			stable_lookup: StableLookup::Found,
			cardano_ok: Query::Panic("stable hash should not query Cardano health"),
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[])
			.await
			.expect("stable MC hash should be valid even when tip looks stale");

		assert_eq!(result, Validation::Success { is_new_best: false });
	}

	#[tokio::test]
	async fn accepts_stable_announcement_even_if_tip_freshness_query_would_fail() {
		let mut validator = validator(TestDataSource {
			tip_fresh: Query::Error("tip freshness query failed"),
			stable_lookup: StableLookup::Found,
			cardano_ok: Query::Panic("stable hash should not query Cardano health"),
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[])
			.await
			.expect("stable MC hash should be valid without querying tip freshness");

		assert_eq!(result, Validation::Success { is_new_best: false });
	}

	#[tokio::test]
	async fn rejects_not_stable_announcement_when_tip_is_fresh() {
		let mut validator = validator(TestDataSource {
			stable_lookup: StableLookup::NotStable,
			cardano_ok: Query::Panic("not-stable with fresh tip should not query Cardano health"),
			..Default::default()
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[])
			.await
			.expect("fresh tip can classify not-yet-stable hash");

		assert_eq!(result, Validation::Failure { disconnect: false });
	}

	#[tokio::test]
	async fn skips_not_stable_announcement_when_tip_is_stale() {
		let mut validator = validator(TestDataSource {
			tip_fresh: Query::Value(false),
			stable_lookup: StableLookup::NotStable,
			cardano_ok: Query::Panic("stale tip should skip before checking Cardano health"),
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn rejects_timestamp_out_of_range_announcement_when_tip_is_fresh() {
		let mut validator = validator(TestDataSource {
			stable_lookup: StableLookup::TimestampOutOfRange,
			cardano_ok: Query::Panic(
				"timestamp-out-of-range with fresh tip should not query Cardano health",
			),
			..Default::default()
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[])
			.await
			.expect("fresh tip can classify timestamp-out-of-range hash");

		assert_eq!(result, Validation::Failure { disconnect: false });
	}

	#[tokio::test]
	async fn skips_timestamp_out_of_range_announcement_when_tip_is_stale() {
		let mut validator = validator(TestDataSource {
			tip_fresh: Query::Value(false),
			stable_lookup: StableLookup::TimestampOutOfRange,
			cardano_ok: Query::Panic("stale tip should skip before checking Cardano health"),
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn rejects_announcement_when_mc_hash_is_unknown_and_cardano_is_ok() {
		let mut validator = validator(TestDataSource {
			stable_lookup: StableLookup::NotFound,
			cardano_ok: Query::Value(true),
			tip_fresh: Query::Panic("block-not-found should not query tip freshness"),
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[])
			.await
			.expect("healthy Cardano view can classify unknown hash");

		assert_eq!(result, Validation::Failure { disconnect: false });
	}

	#[tokio::test]
	async fn skips_announcement_when_mc_hash_is_unknown_and_cardano_is_not_ok() {
		let mut validator = validator(TestDataSource {
			stable_lookup: StableLookup::NotFound,
			cardano_ok: Query::Value(false),
			tip_fresh: Query::Panic("block-not-found should not query tip freshness"),
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn rejects_announcement_with_non_empty_announce_data() {
		let mut validator = validator(TestDataSource::no_queries_expected());

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[1])
			.await
			.expect("non-empty announce data is a peer validation failure");

		assert_eq!(result, Validation::Failure { disconnect: true });
	}

	#[tokio::test]
	async fn rejects_announcement_with_missing_mc_hash_digest() {
		let mut validator = validator(TestDataSource::no_queries_expected());
		let header = Header::new(
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Digest { logs: vec![] },
		);

		let result = validate(&mut validator, &header, &[])
			.await
			.expect("missing MC hash digest is a peer validation failure");

		assert_eq!(result, Validation::Failure { disconnect: true });
	}

	#[tokio::test]
	async fn rejects_announcement_with_malformed_mc_hash_digest() {
		let mut validator = validator(TestDataSource::no_queries_expected());
		let header = Header::new(
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Digest {
				logs: vec![sp_runtime::DigestItem::PreRuntime(MC_HASH_DIGEST_ID, vec![7; 31])],
			},
		);

		let result = validate(&mut validator, &header, &[])
			.await
			.expect("malformed MC hash digest is a peer validation failure");

		assert_eq!(result, Validation::Failure { disconnect: true });
	}

	#[tokio::test]
	async fn rejects_announcement_with_missing_reference_timestamp() {
		let mut validator = validator(TestDataSource::no_queries_expected());
		let header = Header::new(
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Digest { logs: McHashInherentDigest::from_mc_block_hash(McBlockHash([7; 32])) },
		);

		let result = validate(&mut validator, &header, &[])
			.await
			.expect("missing reference timestamp is a peer validation failure");

		assert_eq!(result, Validation::Failure { disconnect: true });
	}

	#[tokio::test]
	async fn skips_announcement_when_tip_freshness_query_fails_for_not_stable_hash() {
		let mut validator = validator(TestDataSource {
			tip_fresh: Query::Error("tip freshness query failed"),
			stable_lookup: StableLookup::NotStable,
			cardano_ok: Query::Panic("tip freshness error should skip before Cardano health"),
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn skips_announcement_when_stable_lookup_reports_local_data_unavailable() {
		let mut validator = validator(TestDataSource {
			stable_lookup: StableLookup::LocalDataUnavailable,
			..Default::default()
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn skips_announcement_when_stable_lookup_fails() {
		let mut validator =
			validator(TestDataSource { stable_lookup: StableLookup::Error, ..Default::default() });

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn skips_announcement_when_cardano_ok_query_fails_after_unknown_hash() {
		let mut validator = validator(TestDataSource {
			stable_lookup: StableLookup::NotFound,
			cardano_ok: Query::Error("Cardano health query failed"),
			tip_fresh: Query::Panic("block-not-found should not query tip freshness"),
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	async fn validate(
		validator: &mut McHashBlockAnnounceValidator<
			SlotBasedBlockReferenceTimestamp<fn(&Header) -> Option<Slot>>,
		>,
		header: &Header,
		data: &[u8],
	) -> Result<Validation, Box<dyn Error + Send>> {
		<McHashBlockAnnounceValidator<_> as BlockAnnounceValidator<Block>>::validate(
			validator, header, data,
		)
		.await
	}

	fn validator(
		source: TestDataSource,
	) -> McHashBlockAnnounceValidator<SlotBasedBlockReferenceTimestamp<fn(&Header) -> Option<Slot>>>
	{
		McHashBlockAnnounceValidator::new(
			Arc::new(source),
			SlotBasedBlockReferenceTimestamp::new(TEST_SLOT_DURATION, test_slot_from_header),
		)
	}

	fn test_slot_from_header(header: &Header) -> Option<Slot> {
		for item in header.digest().logs() {
			if let DigestItem::PreRuntime(id, data) = item
				&& *id == TEST_CONSENSUS_ENGINE_ID
			{
				let bytes: [u8; 8] = data.as_slice().try_into().ok()?;
				return Some(Slot::from(u64::from_le_bytes(bytes)));
			}
		}
		None
	}

	fn query_bool(query: Query<bool>) -> Result<bool, Box<dyn Error + Send + Sync>> {
		match query {
			Query::Value(value) => Ok(value),
			Query::Error(message) => Err(Box::new(io::Error::other(message))),
			Query::Panic(message) => unreachable!("{message}"),
		}
	}

	fn mock_header(mc_hash: McBlockHash) -> Header {
		let mut logs = vec![DigestItem::PreRuntime(
			TEST_CONSENSUS_ENGINE_ID,
			TEST_SLOT.to_le_bytes().to_vec(),
		)];
		logs.extend(McHashInherentDigest::from_mc_block_hash(mc_hash));
		Header::new(
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Digest { logs },
		)
	}

	fn mainchain_block(hash: McBlockHash) -> MainchainBlock {
		MainchainBlock {
			number: McBlockNumber(1),
			hash,
			epoch: McEpochNumber(1),
			slot: McSlotNumber(1),
			timestamp: 1,
		}
	}
}
