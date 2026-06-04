// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::{
	Array, BuildContractAction, BuilderContext, ContractAction, ContractAddress, ContractEffects,
	DB, DUST_EXPECTED_FILES, DustResolver, FetchMode, Intent, KeyLocation, MidnightDataProvider,
	OutputMode, PUBLIC_PARAMS, PedersenRandomness, ProofPreimageMarker, ProvingKeyMaterial,
	Resolver, Signature, StdRng, Timestamp, UnshieldedOfferInfo, deserialize,
	transaction_signing_key,
};
use async_trait::async_trait;
use rand::{CryptoRng, Rng};
use sha2::{Digest, Sha256};
use std::{
	io,
	path::Path,
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

/// A parsed canonical contract key location:
/// `contract:<contract-address-hex>/<circuitId>?vk=<sha-256 of the deployed verifier key, hex>`.
///
/// This is the prover-side routing convention written by the JS transaction assemblers
/// (`compact-js-command` and midnight-js; the grammar is defined in `compact-js`'s
/// `ContractKeyLocation` module). Circuit names alone are ambiguous across contracts, so each
/// contract call's proof preimage embeds the SHA-256 of the call's deployed verifier key and
/// resolvers select the artifact bundle whose verifier key matches *by content*. Protocol builtin
/// locations (`midnight/...`) and legacy bare circuit names do not parse as contract key
/// locations.
pub struct ContractKeyLocation {
	pub contract_address: String,
	pub circuit_id: String,
	pub verifier_key_hash: String,
}

/// Parses a canonical contract key location, returning `None` for any other location form.
pub fn parse_contract_key_location(loc: &str) -> Option<ContractKeyLocation> {
	let rest = loc.strip_prefix("contract:")?;
	let (contract_address, rest) = rest.split_once('/')?;
	let (circuit_id, verifier_key_hash) = rest.split_once("?vk=")?;
	let is_hex_address =
		!contract_address.is_empty() && contract_address.chars().all(|c| c.is_ascii_hexdigit());
	// The circuit identifier is used to build filesystem paths; restrict it to identifier
	// characters (Compact circuit names) so a malicious location cannot traverse directories.
	let is_safe_circuit = !circuit_id.is_empty()
		&& circuit_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
	let is_vk_hash = verifier_key_hash.len() == 64
		&& verifier_key_hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
	(is_hex_address && is_safe_circuit && is_vk_hash).then(|| ContractKeyLocation {
		contract_address: contract_address.to_owned(),
		circuit_id: circuit_id.to_owned(),
		verifier_key_hash: verifier_key_hash.to_owned(),
	})
}

/// Resolves the proving key material for a canonical contract key location by joining on the
/// verifier key: selects the artifact dir whose `keys/<circuit>.verifier` content hashes to the
/// location's embedded value. Immune to circuit-name collisions across contracts, redeploys, and
/// stale artifacts — a bundle is chosen if and only if its proofs verify against the deployed key.
///
/// Fails with a named error when no bundle matches (local artifacts missing or stale with respect
/// to the deployed contract) rather than silently proving with the wrong key material.
pub fn resolve_contract_key_material(
	artifact_dirs: &[String],
	location: &ContractKeyLocation,
) -> std::io::Result<ProvingKeyMaterial> {
	for parent_dir in artifact_dirs {
		let vk_path = format!("{parent_dir}/keys/{}.verifier", location.circuit_id);
		let verifier_key = match std::fs::read(&vk_path) {
			Err(e) if e.kind() == io::ErrorKind::NotFound => {
				log::debug!("Resolver: no verifier key at path {vk_path}");
				continue;
			},
			Err(e) => {
				log::error!("Resolver: error reading verifier key at path {vk_path}: {e}");
				return Err(e);
			},
			Ok(v) => v,
		};
		if hex::encode(Sha256::digest(&verifier_key)) != location.verifier_key_hash {
			log::debug!(
				"Resolver: verifier key at {vk_path} does not match the deployed key for contract '{}'",
				location.contract_address
			);
			continue;
		}
		log::debug!("Resolver: verifier key content match in {parent_dir}");
		let read_bundle_file = |sub_dir: &str, ext: &str| {
			let path = format!("{parent_dir}/{sub_dir}/{}.{ext}", location.circuit_id);
			std::fs::read(&path).map_err(|e| {
				io::Error::new(
					e.kind(),
					format!(
						"artifact bundle at '{parent_dir}' matches the deployed verifier key for \
						 circuit '{}' but '{path}' could not be read: {e}",
						location.circuit_id
					),
				)
			})
		};
		return Ok(ProvingKeyMaterial {
			prover_key: read_bundle_file("keys", "prover")?,
			verifier_key,
			ir_source: read_bundle_file("zkir", "bzkir")?,
		});
	}
	Err(io::Error::new(
		io::ErrorKind::NotFound,
		format!(
			"no ZK artifact bundle matches the deployed verifier key for contract '{}', circuit \
			 '{}': the local compiled artifacts are missing or stale",
			location.contract_address, location.circuit_id
		),
	))
}

pub type SegmentId = u16;

type IntentOf<D> = Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>;
#[async_trait]
pub trait BuildIntent<D: DB + Clone, C: BuilderContext<D>>: Send + Sync {
	async fn build(
		&mut self,
		rng: &mut StdRng,
		ttl: Timestamp,
		context: Arc<C>,
		segment_id: SegmentId,
	) -> IntentOf<D>;
}

pub struct IntentInfo<D: DB + Clone, C: BuilderContext<D>> {
	pub guaranteed_unshielded_offer: Option<UnshieldedOfferInfo<D, C>>,
	pub fallible_unshielded_offer: Option<UnshieldedOfferInfo<D, C>>,
	pub actions: Vec<Box<dyn BuildContractAction<D, C>>>,
	// TODO: Add TTL Option here
}

#[async_trait]
impl<D: DB + Clone, C: BuilderContext<D>> BuildIntent<D, C> for IntentInfo<D, C> {
	async fn build(
		&mut self,
		rng: &mut StdRng,
		ttl: Timestamp,
		context: Arc<C>,
		segment_id: SegmentId,
	) -> IntentOf<D> {
		let mut intent = Intent::<Signature, _, _, _>::empty(rng, ttl);

		for action in self.actions.iter_mut() {
			let next = action.build(rng, context.clone(), &intent).await;
			intent = next;
		}

		let mut guaranteed_signing_keys = Vec::default();
		let mut fallible_signing_keys = Vec::default();
		let dust_registration_signing_keys = Vec::default();

		if let Some(ref guaranteed_unshielded_offer) = self.guaranteed_unshielded_offer {
			let unshielded_offer = guaranteed_unshielded_offer.build(context.clone()).await;
			let signing_keys = guaranteed_unshielded_offer
				.inputs
				.iter()
				.map(|input| input.signing_key(context.clone()))
				.collect::<Vec<_>>();
			intent.guaranteed_unshielded_offer = Some(unshielded_offer);
			guaranteed_signing_keys = signing_keys;
		}

		if let Some(ref fallible_unshielded_offer) = self.fallible_unshielded_offer {
			let unshielded_offer = fallible_unshielded_offer.build(context.clone()).await;
			let signing_keys = fallible_unshielded_offer
				.inputs
				.iter()
				.map(|input| input.signing_key(context.clone()))
				.collect::<Vec<_>>();
			intent.fallible_unshielded_offer = Some(unshielded_offer);
			fallible_signing_keys = signing_keys;
		}

		let guaranteed_signing_keys =
			guaranteed_signing_keys.iter().map(transaction_signing_key).collect::<Vec<_>>();
		let fallible_signing_keys =
			fallible_signing_keys.iter().map(transaction_signing_key).collect::<Vec<_>>();

		intent
			.sign(
				rng,
				segment_id,
				guaranteed_signing_keys.as_slice(),
				fallible_signing_keys.as_slice(),
				dust_registration_signing_keys.as_slice(),
			)
			.unwrap_or_else(|_| panic!("Intent signing with segment_id {segment_id:?} failed"))
	}
}

#[derive(Clone)]
pub struct IntentCustom<D: DB + Clone> {
	pub intent: IntentOf<D>,
	pub resolver: &'static Resolver,
}

impl<D: DB + Clone> IntentCustom<D> {
	/// Maximum file size for intent files (64 MB)
	const MAX_INTENT_FILE_SIZE: u64 = 64 * 1024 * 1024;

	pub fn new_from_file(
		path: impl AsRef<Path>,
		resolver: &'static Resolver,
	) -> Result<Self, std::io::Error> {
		let metadata = std::fs::metadata(path.as_ref())?;
		if metadata.len() > Self::MAX_INTENT_FILE_SIZE {
			return Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				format!("intent file exceeds maximum size of {} bytes", Self::MAX_INTENT_FILE_SIZE),
			));
		}
		let bytes = std::fs::read(path)?;
		let intent: IntentOf<D> = deserialize(bytes.as_slice())?;
		Ok(Self { intent, resolver })
	}

	pub fn new_from_actions<R: Rng + CryptoRng + ?Sized>(
		rng: &mut R,
		actions: &[ContractAction<ProofPreimageMarker, D>],
		resolver: &'static Resolver,
	) -> Self {
		let now = Timestamp::from_secs(
			SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.expect("time has run backwards")
				.as_secs(),
		);
		let intent = Intent {
			guaranteed_unshielded_offer: None,
			fallible_unshielded_offer: None,
			actions: Array::new_from_slice(actions),
			dust_actions: None,
			ttl: now,
			binding_commitment: rng.r#gen(),
		};
		Self { intent, resolver }
	}

	pub fn find_effects(&self) -> (Vec<ContractEffects<D>>, Vec<ContractEffects<D>>) {
		let mut guaranteed_effects = vec![];
		let mut fallible_effects = vec![];
		for action in self.intent.actions.iter() {
			if let ContractAction::Call(ref c) = *action.clone() {
				if let Some(ref t) = c.guaranteed_transcript {
					guaranteed_effects.push(t.effects.clone());
				}
				if let Some(ref t) = c.fallible_transcript {
					fallible_effects.push(t.effects.clone());
				}
			}
		}
		(guaranteed_effects, fallible_effects)
	}

	pub fn find_contract_address(&self) -> Option<ContractAddress> {
		self.intent.actions.iter().find_map(|action| match *action {
			ContractAction::Call(ref c) => Some(c.address),
			ContractAction::Maintain(ref c) => Some(c.address),
			_ => None,
		})
	}

	pub fn get_resolver(artifact_dirs: &[String]) -> Result<Resolver, std::io::Error> {
		let artifact_dirs = artifact_dirs.to_vec();
		Ok(Resolver::new(
			PUBLIC_PARAMS.clone(),
			DustResolver(MidnightDataProvider::new(
				FetchMode::OnDemand,
				OutputMode::Log,
				DUST_EXPECTED_FILES.to_owned(),
			)?),
			Box::new(move |KeyLocation(loc)| {
				let artifact_dirs = artifact_dirs.to_vec();
				let sync_block = move || {
					// Canonical contract key locations (written by the JS transaction assemblers)
					// resolve by verifier-key content, which is immune to circuit-name collisions
					// across contracts. Any other location is a legacy bare circuit name from a
					// toolkit-internal builder and falls through to the historical
					// first-match-wins filename scan below.
					if let Some(location) = parse_contract_key_location(&loc) {
						return resolve_contract_key_material(&artifact_dirs, &location).map(Some);
					}
					let read_file = |dir, ext| {
						for parent_dir in &artifact_dirs {
							let path = format!("{parent_dir}/{dir}/{loc}.{ext}");
							match std::fs::read(&path) {
								Err(e) if e.kind() == io::ErrorKind::NotFound => {
									log::debug!("Resolver: missing key at path {path}");
									continue;
								},
								Err(e) => {
									log::error!("Resolver: error reading key at path {path}: {e}");
									return Err(e);
								},
								Ok(v) => {
									log::debug!("Resolver: found key at path {path}");
									return Ok(Some(v));
								},
							}
						}
						Ok(None)
					};
					let Some(prover_key) = read_file("keys", "prover")? else {
						log::warn!("prover key not created");
						return Ok(None);
					};
					let Some(verifier_key) = read_file("keys", "verifier")? else {
						log::warn!("verifier key not created");
						return Ok(None);
					};
					let Some(ir_source) = read_file("zkir", "bzkir")? else {
						log::warn!("IR source not created");
						return Ok(None);
					};

					log::info!("Creating Proving Key Material...");

					Ok(Some(ProvingKeyMaterial { prover_key, verifier_key, ir_source }))
				};
				let res = sync_block();
				Box::pin(std::future::ready(res))
			}),
		))
	}
}

#[async_trait]
impl<D: DB + Clone, C: BuilderContext<D>> BuildIntent<D, C> for IntentCustom<D> {
	async fn build(
		&mut self,
		_rng: &mut StdRng,
		ttl: Timestamp,
		context: Arc<C>,
		_segment_id: SegmentId,
	) -> IntentOf<D> {
		log::debug!("Updating the resolver...");
		context.update_resolver(self.resolver).await;
		let mut intent = self.intent.clone();
		intent.ttl = ttl;
		intent
	}
}

#[async_trait]
impl<D: DB + Clone, C: BuilderContext<D>> BuildContractAction<D, C> for IntentCustom<D> {
	async fn build(
		&mut self,
		_rng: &mut StdRng,
		context: Arc<C>,
		intent: &Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>,
	) -> Intent<Signature, ProofPreimageMarker, PedersenRandomness, D> {
		let mut actions = intent.actions.clone();

		for action in self.intent.actions.iter() {
			actions = actions.push((*action).clone());
		}

		let result = IntentOf::<D> {
			guaranteed_unshielded_offer: intent.guaranteed_unshielded_offer.clone(),
			fallible_unshielded_offer: intent.fallible_unshielded_offer.clone(),
			actions,
			dust_actions: intent.dust_actions.clone(),
			ttl: intent.ttl,
			binding_commitment: intent.binding_commitment,
		};

		context.update_resolver(self.resolver).await;
		result
	}
}

#[cfg(test)]
mod contract_key_location_tests {
	use super::{parse_contract_key_location, resolve_contract_key_material};
	use sha2::{Digest, Sha256};
	use std::io::ErrorKind;

	const ADDRESS: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

	fn vk_hash(bytes: &[u8]) -> String {
		hex::encode(Sha256::digest(bytes))
	}

	#[test]
	fn parses_canonical_locations() {
		let hash = "a".repeat(64);
		let location =
			parse_contract_key_location(&format!("contract:{ADDRESS}/transfer?vk={hash}"))
				.expect("should parse");
		assert_eq!(location.contract_address, ADDRESS);
		assert_eq!(location.circuit_id, "transfer");
		assert_eq!(location.verifier_key_hash, hash);
	}

	#[test]
	fn rejects_non_canonical_locations() {
		let hash = "a".repeat(64);
		for loc in [
			"midnight/zswap/spend".to_owned(),
			"transfer".to_owned(),
			"dummy".to_owned(),
			format!("contract:not-hex/transfer?vk={hash}"),
			format!("contract:{ADDRESS}/transfer"),
			format!("contract:{ADDRESS}/a/b?vk={hash}"),
			format!("contract:{ADDRESS}/../escape?vk={hash}"),
			format!("contract:{ADDRESS}/transfer?vk={}", "A".repeat(64)),
			format!("contract:{ADDRESS}/transfer?vk={}", "a".repeat(63)),
		] {
			assert!(parse_contract_key_location(&loc).is_none(), "should not parse: {loc}");
		}
	}

	fn write_bundle(dir: &std::path::Path, circuit: &str, vk: &[u8], pk: &[u8], ir: &[u8]) {
		std::fs::create_dir_all(dir.join("keys")).unwrap();
		std::fs::create_dir_all(dir.join("zkir")).unwrap();
		std::fs::write(dir.join("keys").join(format!("{circuit}.verifier")), vk).unwrap();
		std::fs::write(dir.join("keys").join(format!("{circuit}.prover")), pk).unwrap();
		std::fs::write(dir.join("zkir").join(format!("{circuit}.bzkir")), ir).unwrap();
	}

	fn temp_dirs(name: &str) -> (tempfile::TempDir, String, String) {
		let root = tempfile::Builder::new().prefix(name).tempdir().unwrap();
		let a = root.path().join("contract-a").to_string_lossy().into_owned();
		let b = root.path().join("contract-b").to_string_lossy().into_owned();
		(root, a, b)
	}

	#[test]
	fn selects_the_bundle_whose_verifier_key_matches_by_content() {
		// Two contracts both define a circuit named 'transfer' with different keys — the
		// collision case content addressing exists to disambiguate.
		let (_root, dir_a, dir_b) = temp_dirs("ckl-select");
		write_bundle(std::path::Path::new(&dir_a), "transfer", b"vk-a", b"pk-a", b"ir-a");
		write_bundle(std::path::Path::new(&dir_b), "transfer", b"vk-b", b"pk-b", b"ir-b");
		let dirs = vec![dir_a, dir_b];

		let location = parse_contract_key_location(&format!(
			"contract:{ADDRESS}/transfer?vk={}",
			vk_hash(b"vk-b")
		))
		.unwrap();
		let material = resolve_contract_key_material(&dirs, &location).unwrap();

		assert_eq!(material.verifier_key, b"vk-b");
		assert_eq!(material.prover_key, b"pk-b");
		assert_eq!(material.ir_source, b"ir-b");
	}

	#[test]
	fn fails_with_a_named_error_when_no_verifier_key_matches() {
		let (_root, dir_a, _) = temp_dirs("ckl-drift");
		write_bundle(std::path::Path::new(&dir_a), "transfer", b"vk-a", b"pk-a", b"ir-a");

		let location = parse_contract_key_location(&format!(
			"contract:{ADDRESS}/transfer?vk={}",
			vk_hash(b"some-other-deployed-vk")
		))
		.unwrap();
		let error = resolve_contract_key_material(&[dir_a], &location).unwrap_err();

		assert_eq!(error.kind(), ErrorKind::NotFound);
		assert!(
			error.to_string().contains("no ZK artifact bundle matches the deployed verifier key"),
			"unexpected error: {error}"
		);
	}

	#[test]
	fn fails_when_a_matching_bundle_is_missing_its_prover_key() {
		let (_root, dir_a, _) = temp_dirs("ckl-partial");
		write_bundle(std::path::Path::new(&dir_a), "transfer", b"vk-a", b"pk-a", b"ir-a");
		std::fs::remove_file(
			std::path::Path::new(&dir_a).join("keys").join("transfer.prover"),
		)
		.unwrap();

		let location = parse_contract_key_location(&format!(
			"contract:{ADDRESS}/transfer?vk={}",
			vk_hash(b"vk-a")
		))
		.unwrap();
		let error = resolve_contract_key_material(&[dir_a], &location).unwrap_err();

		assert!(error.to_string().contains("could not be read"), "unexpected error: {error}");
	}
}
