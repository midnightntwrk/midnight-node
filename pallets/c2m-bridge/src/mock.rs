use frame_support::{construct_runtime, derive_impl, traits::ConstU32};
use frame_system::EnsureRoot;
use midnight_node_ledger::types::Hash;
use midnight_primitives::MidnightSystemTransactionExecutor;
use sp_io::TestExternalities;
use sp_runtime::{AccountId32, BuildStorage};

pub type Block = frame_system::mocking::MockBlock<Test>;
pub type AccountId = AccountId32;
pub type MaxTxLength = ConstU32<1024>;

pub const TEST_MINIMAL_TRANSFER_CSTARS: u128 = 99;

#[frame_support::pallet]
pub mod mock_pallet {
	use super::*;
	use crate::MinBridgeAmountProvider;
	use frame_support::pallet_prelude::*;
	use midnight_node_ledger::latest::api::LedgerApiError;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::storage]
	#[pallet::unbounded]
	pub type Transfers<T: Config> = StorageValue<_, Vec<BoundedVec<u8, MaxTxLength>>, ValueQuery>;

	#[pallet::storage]
	pub type TransfersCount<T: Config> = StorageValue<_, u8, ValueQuery>;

	impl<T> MidnightSystemTransactionExecutor for Pallet<T> {
		fn execute_system_transaction(tx: Vec<u8>) -> Result<Hash, DispatchError> {
			let bounded_vec: BoundedVec<u8, MaxTxLength> = tx.clone().try_into().unwrap();
			Transfers::<Test>::append(bounded_vec);
			let count = TransfersCount::<Test>::get();
			TransfersCount::<Test>::put(count + 1);
			Ok([count; 32])
		}
	}

	impl<T> MinBridgeAmountProvider for Pallet<T> {
		// Returns value in STARS, pallet denominates it to cNIGHT
		fn get_c_to_m_bridge_min_amount() -> Result<u128, LedgerApiError> {
			Ok(TEST_MINIMAL_TRANSFER_CSTARS)
		}
	}
}

construct_runtime! {
	pub enum Test {
		System: frame_system,
		C2MBridge: crate::pallet,
		Mock: crate::mock::mock_pallet
	}
}

impl mock_pallet::Config for Test {}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl crate::Config for Test {
	type GovernanceOrigin = EnsureRoot<AccountId>;
	type MidnightSystemTransactionExecutor = Mock;
	type MinBridgeAmountProvider = Mock;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t: TestExternalities =
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into();
	// Frame system drops events from block 0
	t.execute_with(|| {
		frame_system::Pallet::<Test>::set_block_number(1);
	});
	t
}
