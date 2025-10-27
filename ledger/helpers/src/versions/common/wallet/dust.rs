use thiserror::Error;

use super::super::{
	DB, DerivationPath, DeriveSeed, Deserializable, DustLocalState, DustNullifier, DustOutput,
	DustParameters, DustPublicKey, DustSecretKey, DustSpend, Event, EventReplayError, HRP_CONSTANT,
	HRP_CREDENTIAL_DUST, HashSet, IntoWalletAddress, LedgerParameters, MnLedgerDustSpendError,
	ProofPreimageMarker, QualifiedDustOutput, Role, Serializable, ShortTaggedDeserializeError,
	Tagged, Timestamp, WalletAddress, WalletSeed, short_tagged_deserialize, short_tagged_serialize,
};

#[derive(Debug, Clone)]
pub struct DustWallet<D: DB> {
	pub public_key: DustPublicKey,
	secret_key: Option<DustSecretKey>,
	pub dust_local_state: Option<DustLocalState<D>>,
	// We track the UTXOs we spent, to avoid spending the same UTXO twice in one batch of TXs.
	// This set is cleared in `process_ttls`, because that is called when a new block is produced.
	spent_utxos: HashSet<DustNullifier>,
}

impl<D: DB> DeriveSeed for DustWallet<D> {}

#[cfg(feature = "can-panic")]
impl<D: DB> IntoWalletAddress for DustWallet<D> {
	fn address(&self, network_id: &str) -> WalletAddress {
		let hrp_string =
			format!("{HRP_CONSTANT}_{HRP_CREDENTIAL_DUST}{}", Self::network_suffix(network_id));
		let hrp = bech32::Hrp::parse(&hrp_string)
			.unwrap_or_else(|err| panic!("Error while bech32 parsing: {err}"));

		let address = DustAddress { public_key: self.public_key };
		let data = short_tagged_serialize(&address);
		WalletAddress::new(hrp, data)
	}
}

impl<D: DB> DustWallet<D> {
	fn from_seed(derived_seed: [u8; 32], params: Option<&LedgerParameters>) -> Self {
		let secret_key = DustSecretKey::derive_secret_key(&derived_seed);
		let public_key = secret_key.clone().into();
		let dust_local_state = params.map(|p| DustLocalState::new(p.dust));
		let spent_utxos = HashSet::new();
		Self { public_key, secret_key: Some(secret_key), dust_local_state, spent_utxos }
	}

	pub fn default(root_seed: WalletSeed, params: Option<&LedgerParameters>) -> Self {
		let role = Role::Dust;
		let path = DerivationPath::default_for_role(role);
		let derived_seed = Self::derive_seed(root_seed, &path);

		Self::from_seed(derived_seed, params)
	}

	pub fn from_path(
		root_seed: WalletSeed,
		path: &DerivationPath,
		params: Option<&LedgerParameters>,
	) -> Self {
		let derived_seed = Self::derive_seed(root_seed, path);

		Self::from_seed(derived_seed, params)
	}

	pub fn replay_events(&mut self, events: &[Event<D>]) -> Result<(), EventReplayError> {
		if let Some(state) = self.dust_local_state.as_mut()
			&& let Some(sk) = self.secret_key.as_ref()
		{
			*state = state.clone().replay_events(sk, events)?;
		}
		Ok(())
	}

	pub fn process_ttls(&mut self, tblock: Timestamp) {
		if let Some(state) = self.dust_local_state.as_mut() {
			*state = state.clone().process_ttls(tblock);
		}
		self.spent_utxos = HashSet::new()
	}

	pub fn speculative_spend(
		&self,
		amount: u128,
		ctime: Timestamp,
		params: &DustParameters,
	) -> Result<Vec<DustSpend<ProofPreimageMarker, D>>, DustSpendError> {
		let Some(original_state) = self.dust_local_state.as_ref() else {
			return Err(DustSpendError::MissingLocalState);
		};
		let Some(sk) = self.secret_key.as_ref() else {
			return Err(DustSpendError::MissingLocalState);
		};
		let mut spends = vec![];
		let mut remaining_amount = amount;
		let mut state = original_state.clone();
		for qdo in original_state.utxos() {
			if self.spent_utxos.member(&qdo.nullifier(sk)) {
				continue;
			}
			let Some(gen_info) = state.generation_info(&qdo) else {
				return Err(DustSpendError::UnrecognizedDustOutput(Box::new(qdo)));
			};
			let output_amount_now = DustOutput::from(qdo).updated_value(&gen_info, ctime, params);
			let v_fee = remaining_amount.min(output_amount_now);
			if v_fee == 0 {
				continue;
			}
			let (new_state, spend) = state
				.spend(sk, &qdo, v_fee, ctime)
				.map_err(|e| DustSpendError::Internal(Box::new(e)))?;
			state = new_state;
			spends.push(spend);
			remaining_amount -= v_fee;
			if remaining_amount == 0 {
				break;
			}
		}
		Ok(spends)
	}

	pub fn mark_spent(&mut self, spends: &[DustSpend<ProofPreimageMarker, D>]) {
		for spend in spends {
			self.spent_utxos = self.spent_utxos.insert(spend.old_nullifier);
		}
	}
}

#[derive(Serializable)]
#[tag = "dust-address[v1]"]
struct DustAddress {
	public_key: DustPublicKey,
}

pub enum DustAddressParseError {
	DecodeError(bech32::DecodeError),
	InvalidHrpPrefix,
	InvalidHrpCredential,
	AddressNotDust,
	Deserialize(ShortTaggedDeserializeError),
}

impl<D: DB> TryFrom<&WalletAddress> for DustWallet<D> {
	type Error = DustAddressParseError;

	fn try_from(address: &WalletAddress) -> Result<Self, Self::Error> {
		let hrp = address.human_readable_part();
		let data = address.data();

		let prefix_parts = hrp.as_str().split('_').collect::<Vec<&str>>();

		prefix_parts
			.first()
			.filter(|c| *c == &HRP_CONSTANT)
			.ok_or(DustAddressParseError::InvalidHrpPrefix)?;

		let hrp_credential = prefix_parts
			.get(1)
			.ok_or(DustAddressParseError::InvalidHrpCredential)?
			.to_string();

		if hrp_credential != HRP_CREDENTIAL_DUST {
			return Err(DustAddressParseError::AddressNotDust);
		}

		let dust_address: DustAddress =
			short_tagged_deserialize(data).map_err(DustAddressParseError::Deserialize)?;
		Ok(DustWallet {
			public_key: dust_address.public_key,
			secret_key: None,
			dust_local_state: None,
			spent_utxos: HashSet::new(),
		})
	}
}

#[derive(Debug, Error)]
pub enum DustSpendError {
	#[error("This wallet was not initialized with all required data")]
	MissingLocalState,
	#[error("Unrecognized dust output {0:?}")]
	UnrecognizedDustOutput(Box<QualifiedDustOutput>),
	#[error("{0}")]
	Internal(Box<MnLedgerDustSpendError>),
}
