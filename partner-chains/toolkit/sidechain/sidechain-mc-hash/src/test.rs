mod inherent_digest_tests {
	use crate::mock::*;
	use crate::*;
	use sp_partner_chains_consensus::InherentDigest;

	#[tokio::test]
	async fn from_inherent_data_works() {
		let inherent_data = MockMcHashInherentDataProvider { mc_hash: McBlockHash([42; 32]) }
			.create_inherent_data()
			.await
			.unwrap();

		let result = McHashInherentDigest::from_inherent_data(&inherent_data)
			.expect("from_inherent_data should not fail");

		assert_eq!(result, vec![DigestItem::PreRuntime(MC_HASH_DIGEST_ID, vec![42; 32])])
	}

	#[tokio::test]
	async fn value_from_digest_works() {
		let digest_to_ignore = DigestItem::PreRuntime(*b"irlv", vec![0; 32]);
		let digest = DigestItem::PreRuntime(MC_HASH_DIGEST_ID, vec![42; 32]);

		let result = McHashInherentDigest::value_from_digest(&[digest_to_ignore, digest])
			.expect("value_from_digest should not fail");

		assert_eq!(result, McBlockHash([42; 32]))
	}

	#[tokio::test]
	async fn value_from_digest_rejects_duplicate_mc_hash_entries() {
		let first = DigestItem::PreRuntime(MC_HASH_DIGEST_ID, vec![7; 32]);
		let second = DigestItem::PreRuntime(MC_HASH_DIGEST_ID, vec![8; 32]);

		let err = McHashInherentDigest::value_from_digest(&[first, second])
			.expect_err("duplicate MC hash digests must be rejected");

		assert_eq!(err.to_string(), "Multiple main chain block hashes in digest");
	}
}

mod validation_tests {
	use crate::McHashInherentError::*;
	use crate::mock::MockMcHashDataSource;
	use crate::*;
	use sp_consensus_slots::Slot;
	use sp_consensus_slots::SlotDuration;
	use sp_runtime::testing::Digest;
	use sp_runtime::testing::Header;
	use sp_runtime::traits::Header as HeaderT;

	#[tokio::test]
	async fn mc_state_reference_block_numbers_should_not_decrease() {
		let mc_block_hash = McBlockHash([2; 32]);
		let parent_stable_block_hash = McBlockHash([1; 32]);
		let slot_duration = SlotDuration::from_millis(1000);

		let parent_stable_block = MainchainBlock {
			number: McBlockNumber(1),
			hash: parent_stable_block_hash.clone(),
			epoch: McEpochNumber(2),
			slot: McSlotNumber(3),
			timestamp: 4,
		};

		let next_stable_block = MainchainBlock {
			number: McBlockNumber(parent_stable_block.number.0 - 1),
			hash: mc_block_hash.clone(),
			slot: McSlotNumber(parent_stable_block.slot.0 - 1),
			timestamp: parent_stable_block.timestamp - 1,
			epoch: McEpochNumber(parent_stable_block.epoch.0),
		};
		let mc_hash_data_source =
			MockMcHashDataSource::new(vec![parent_stable_block, next_stable_block], vec![]);

		let err = McHashInherentDataProvider::new_verification(
			mock_header(parent_stable_block_hash),
			Some(Slot::from(1)),
			30.into(),
			mc_block_hash.clone(),
			slot_duration,
			&mc_hash_data_source,
		)
		.await;
		assert!(err.is_err());
		assert_eq!(
			err.unwrap_err().to_string(),
			McStateReferenceRegressed(mc_block_hash, 30.into(), McBlockNumber(0), McBlockNumber(1))
				.to_string()
		);
	}

	#[tokio::test]
	async fn proposed_mc_state_reference_block_numbers_should_not_decrease() {
		let mc_block_hash = McBlockHash([2; 32]);
		let parent_stable_block_hash = McBlockHash([3; 32]);
		let slot_duration = SlotDuration::from_millis(1000);

		let parent_stable_block = MainchainBlock {
			number: McBlockNumber(3),
			hash: parent_stable_block_hash.clone(),
			epoch: McEpochNumber(1),
			slot: McSlotNumber(30),
			timestamp: 300,
		};

		let current_latest_stable_block_from_db_sync = MainchainBlock {
			number: McBlockNumber(2),
			hash: mc_block_hash.clone(),
			slot: McSlotNumber(20),
			epoch: McEpochNumber(1),
			timestamp: 200,
		};

		let mc_hash_data_source = MockMcHashDataSource::new(
			vec![current_latest_stable_block_from_db_sync],
			vec![parent_stable_block.clone()],
		);
		let provider = McHashInherentDataProvider::new_proposal(
			mock_header(parent_stable_block_hash),
			&mc_hash_data_source,
			Slot::from(1),
			slot_duration,
		)
		.await
		.unwrap();
		assert_eq!(provider.mc_block, parent_stable_block);
	}

	#[tokio::test]
	async fn propose_fails_if_parent_mc_state_cannot_be_found() {
		let mc_block_hash = McBlockHash([2; 32]);
		let parent_stable_block_hash = McBlockHash([3; 32]);
		let slot_duration = SlotDuration::from_millis(1000);

		let current_latest_stable_block_from_db_sync = MainchainBlock {
			number: McBlockNumber(2),
			hash: mc_block_hash.clone(),
			slot: McSlotNumber(20),
			epoch: McEpochNumber(1),
			timestamp: 200,
		};

		let mc_hash_data_source =
			MockMcHashDataSource::new(vec![current_latest_stable_block_from_db_sync], vec![]);
		let err = McHashInherentDataProvider::new_proposal(
			mock_header(parent_stable_block_hash),
			&mc_hash_data_source,
			Slot::from(1),
			slot_duration,
		)
		.await
		.unwrap_err();
		assert_eq!(err.to_string(), StableBlockNotFoundByHash(mc_block_hash).to_string());
	}

	pub fn mock_header(mc_hash: McBlockHash) -> Header {
		Header::new(
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Digest { logs: McHashInherentDigest::from_mc_block_hash(mc_hash) },
		)
	}

	/// Convenience: a small synthetic MainchainBlock with the given hash.
	fn make_block(hash: McBlockHash) -> MainchainBlock {
		MainchainBlock {
			number: McBlockNumber(5),
			hash,
			slot: McSlotNumber(50),
			epoch: McEpochNumber(1),
			timestamp: 500,
		}
	}

	#[tokio::test]
	async fn stable_reference_verifies() {
		let slot_duration = SlotDuration::from_millis(1000);
		let mc_block_hash = McBlockHash([7; 32]);
		let block = make_block(mc_block_hash.clone());

		let data_source = MockMcHashDataSource::new(vec![block.clone()], vec![]);

		let provider = McHashInherentDataProvider::new_verification(
			mock_header(McBlockHash([8; 32])),
			None,
			Slot::from(30),
			mc_block_hash.clone(),
			slot_duration,
			&data_source,
		)
		.await
		.expect("a stable reference should verify");

		assert_eq!(provider.mc_hash(), mc_block_hash);
		assert_eq!(provider.mc_block(), block.number);
	}

	#[tokio::test]
	async fn unstable_block_with_fresh_tip_rejects_as_invalid() {
		let slot_duration = SlotDuration::from_millis(1000);
		let mc_block_hash = McBlockHash([7; 32]);
		let unstable = make_block(mc_block_hash.clone());

		let data_source = MockMcHashDataSource::new(vec![], vec![unstable]);
		// Our Cardano tip looks fresh, so an unstable reference is a dishonest one.
		data_source.set_tip_fresh_responses([true]);

		let verified_block_slot = Slot::from(30);
		let err = McHashInherentDataProvider::new_verification(
			mock_header(McBlockHash([8; 32])),
			None,
			verified_block_slot,
			mc_block_hash.clone(),
			slot_duration,
			&data_source,
		)
		.await
		.unwrap_err();

		let timestamp = verified_block_slot.timestamp(slot_duration).unwrap();
		assert_eq!(
			err.to_string(),
			McStateReferenceInvalid(mc_block_hash, verified_block_slot, timestamp).to_string()
		);
	}

	#[tokio::test]
	async fn unstable_block_with_stale_tip_awaits_cardano() {
		let slot_duration = SlotDuration::from_millis(1000);
		let mc_block_hash = McBlockHash([7; 32]);
		let unstable = make_block(mc_block_hash.clone());

		let data_source = MockMcHashDataSource::new(vec![], vec![unstable]);
		// Our Cardano tip looks stale, so we cannot yet rule the reference out: the caller
		// should back off and retry rather than treat it as invalid.
		data_source.set_tip_fresh_responses([false]);

		let err = McHashInherentDataProvider::new_verification(
			mock_header(McBlockHash([8; 32])),
			None,
			Slot::from(30),
			mc_block_hash.clone(),
			slot_duration,
			&data_source,
		)
		.await
		.unwrap_err();

		assert_eq!(err.to_string(), AwaitingCardanoData(mc_block_hash).to_string());
	}

	#[tokio::test]
	async fn unknown_block_with_unhealthy_cardano_awaits_cardano() {
		let slot_duration = SlotDuration::from_millis(1000);
		let mc_block_hash = McBlockHash([7; 32]);

		let data_source = MockMcHashDataSource::new(vec![], vec![]);
		// Unknown reference while our Cardano view is unhealthy: hold off rather than reject.
		data_source.set_cardano_ok_responses([false]);

		let err = McHashInherentDataProvider::new_verification(
			mock_header(McBlockHash([8; 32])),
			None,
			Slot::from(30),
			mc_block_hash.clone(),
			slot_duration,
			&data_source,
		)
		.await
		.unwrap_err();

		assert_eq!(err.to_string(), AwaitingCardanoData(mc_block_hash).to_string());
	}

	#[tokio::test]
	async fn unknown_block_when_cardano_is_ok_is_rejected() {
		let err = McHashInherentDataProvider::new_verification(
			mock_header(McBlockHash([8; 32])),
			None,
			Slot::from(30),
			McBlockHash([7; 32]),
			SlotDuration::from_millis(1000),
			&MockMcHashDataSource::new(vec![], vec![]),
		)
		.await
		.unwrap_err();

		assert_eq!(
			err.to_string(),
			McStateReferenceInvalid(McBlockHash([7; 32]), Slot::from(30), Timestamp::new(30000))
				.to_string()
		);
	}
}
