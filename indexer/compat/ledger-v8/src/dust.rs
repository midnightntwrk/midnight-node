// This file is part of midnight-ledger.
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

use crate::error::{
    DustLocalStateError, DustStateError, EventReplayError, MalformedTransaction, TransactionInvalid,
};
use crate::events::{Event, EventDetails};
use crate::semantics::TransactionContext;
use crate::structure::{
    ErasedIntent, IntentHash, ProofKind, ProofMarker, ProofPreimageMarker, SPECKS_PER_DUST,
    STARS_PER_NIGHT, SignatureKind, Symbol, TransactionHash, UnshieldedOffer, Utxo, UtxoSpend,
    UtxoState,
};
use crate::verify::{StateReference, WellFormedStrictness};
use base_crypto::{
    MemWrite,
    hash::{HashOutput, PERSISTENT_HASH_BYTES, PersistentHashWriter, persistent_commit},
    signatures::VerifyingKey,
    time::{Duration, Timestamp},
};
use base_crypto::{
    data_provider::MidnightDataProvider,
    fab::{Aligned, AlignedValue, Alignment, AlignmentAtom, AlignmentSegment, Value},
};
use coin_structure::coin::{NIGHT, UserAddress};
use derive_where::derive_where;
use futures::future::join_all;
#[cfg(feature = "proof-verifying")]
use lazy_static::lazy_static;
use onchain_runtime::{
    Cell_read, HistoricMerkleTree_check_root, HistoricMerkleTree_insert_hash, Set_insert,
    ops::{Key, Op},
    result_mode::ResultModeGather,
    state::StateValue,
};
use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
#[cfg(feature = "proof-verifying")]
use serialize::tagged_deserialize;
use serialize::{Deserializable, Serializable, Tagged, tag_enforcement_test};
use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter};
#[cfg(test)]
use storage::db::InMemoryDB;
use storage::{
    Storable,
    arena::{ArenaKey, Sp},
    db::DB,
    storable::Loader,
    storage::{HashMap, HashSet, Identity, Map, TimeFilterMap},
};
use transient_crypto::commitment::Pedersen;
use transient_crypto::curve::FR_BYTES;
use transient_crypto::hash::{degrade_to_transient, transient_commit};
use transient_crypto::merkle_tree::MerkleTreeCollapsedUpdate;
#[cfg(feature = "proof-verifying")]
use transient_crypto::proofs::VerifierKey;
use transient_crypto::proofs::{ProvingKeyMaterial, ProvingProvider};
use transient_crypto::{
    curve::Fr,
    hash::{transient_hash, upgrade_from_transient},
    merkle_tree,
    merkle_tree::{MerkleTree, MerkleTreeDigest},
    proofs::{KeyLocation, ProofPreimage, ProvingError, Resolver},
    repr::FieldRepr,
};
use zeroize::{Zeroize, ZeroizeOnDrop};
use zswap::verify::with_outputs;

#[cfg(feature = "proof-verifying")]
const SPEND_VK_RAW: &[u8] = include_bytes!("../static/dust/spend.verifier");

#[cfg(feature = "proof-verifying")]
lazy_static! {
    pub static ref SPEND_VK: VerifierKey =
        tagged_deserialize(&mut SPEND_VK_RAW.to_vec().as_slice())
            .expect("Zswap Output VK should be valid");
}

pub struct DustResolver(pub MidnightDataProvider);

impl Resolver for DustResolver {
    async fn resolve_key(&self, key: KeyLocation) -> std::io::Result<Option<ProvingKeyMaterial>> {
        let file_root = match &*key.0 {
            "midnight/dust/spend" => {
                concat!("dust/", midnight_ledger_static::version!(), "/spend")
            }
            _ => return Ok(None),
        };
        fn read_to_vec(mut reader: impl std::io::Read) -> std::io::Result<Vec<u8>> {
            let mut res = Vec::new();
            reader.read_to_end(&mut res)?;
            Ok(res)
        }
        let prover_key = read_to_vec(
            &mut self
                .0
                .get_file(
                    &format!("{file_root}.prover"),
                    &format!("failed to find built-in dust prover key {file_root}.prover"),
                )
                .await?,
        )?;
        let verifier_key = read_to_vec(
            &mut self
                .0
                .get_file(
                    &format!("{file_root}.verifier"),
                    &format!("failed to find built-in dust verifier key {file_root}.verifier"),
                )
                .await?,
        )?;
        let ir_source = read_to_vec(
            &mut self
                .0
                .get_file(
                    &format!("{file_root}.bzkir"),
                    &format!("failed to find built-in dust IR {file_root}.bzkir"),
                )
                .await?,
        )?;
        Ok(Some(ProvingKeyMaterial {
            ir_source,
            prover_key,
            verifier_key,
        }))
    }
}

const DUST_COMMITMENT_TREE_DEPTH: u8 = 32;
const DUST_GENERATION_TREE_DEPTH: u8 = 32;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serializable, Storable, FieldRepr)]
#[storable(base)]
#[tag = "dust-nullifier[v1]"]
pub struct DustNullifier(pub Fr);
tag_enforcement_test!(DustNullifier);

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Serializable, Storable, FieldRepr)]
#[storable(base)]
#[tag = "dust-commitment[v1]"]
pub struct DustCommitment(pub Fr);
tag_enforcement_test!(DustCommitment);

impl From<DustCommitment> for HashOutput {
    fn from(com: DustCommitment) -> HashOutput {
        upgrade_from_transient(com.0)
    }
}

impl From<HashOutput> for DustCommitment {
    fn from(value: HashOutput) -> Self {
        DustCommitment(degrade_to_transient(value))
    }
}

pub type Seed = [u8; 32];

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serializable, Storable, FieldRepr)]
#[storable(base)]
#[tag = "dust-public-key[v1]"]
pub struct DustPublicKey(pub Fr);
tag_enforcement_test!(DustPublicKey);

#[derive(Clone, Serializable, Storable, FieldRepr, Zeroize, ZeroizeOnDrop)]
#[storable(base)]
#[tag = "dust-secret-key[v1]"]
pub struct DustSecretKey(pub Fr);
tag_enforcement_test!(DustSecretKey);

impl Debug for DustSecretKey {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("<dust secret key>")
    }
}

impl From<DustSecretKey> for DustPublicKey {
    fn from(sk: DustSecretKey) -> DustPublicKey {
        DustPublicKey(transient_hash(&[b"mdn:dust:pk".field_vec()[0], sk.0]))
    }
}

impl DustSecretKey {
    pub fn sample(rng: &mut (impl Rng + CryptoRng)) -> Self {
        DustSecretKey(rng.r#gen())
    }

    pub fn nonces(&self, initial_nonce: InitialNonce) -> impl Iterator<Item = Fr> + use<'_> {
        let pk = DustPublicKey::from(self.clone());
        (0u32..).map(move |seq| {
            transient_hash(&(initial_nonce, seq, if seq == 0 { pk.0 } else { self.0 }).field_vec())
        })
    }

    pub fn derive_secret_key(seed: &Seed) -> Self {
        const DOMAIN_SEPARATOR: &[u8; 12] = b"midnight:dsk";
        const NUMBER_OF_BYTES: usize = 64;
        let raw_bytes = DustSecretKey::sample_bytes(seed, NUMBER_OF_BYTES, DOMAIN_SEPARATOR);
        let raw_bytes_arr: [u8; 64] = raw_bytes.clone().try_into().unwrap();
        DustSecretKey(Fr::from_uniform_bytes(&raw_bytes_arr))
    }

    pub fn sample_bytes(seed: &Seed, no_of_bytes: usize, domain_separator: &[u8]) -> Vec<u8> {
        let hash_bytes = PERSISTENT_HASH_BYTES;
        let rounds = no_of_bytes.div_ceil(hash_bytes);
        let mut res: Vec<u8> = Vec::new();
        for round in 0..rounds {
            let mut outer_writer = PersistentHashWriter::new();
            MemWrite::write(&mut outer_writer, domain_separator);
            MemWrite::write(&mut outer_writer, &{
                let mut inner_writer = PersistentHashWriter::new();
                MemWrite::write(&mut inner_writer, &((round as u64).to_le_bytes()));
                MemWrite::write(&mut inner_writer, seed);
                inner_writer.finalize().0
            });
            let round_hash = outer_writer.finalize();
            let bytes_to_add = hash_bytes.min(no_of_bytes - round * 32);
            res.extend_from_slice(&round_hash.0[0..bytes_to_add])
        }
        res
    }

    pub fn repr(&self) -> Vec<u8> {
        self.0.as_le_bytes()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serializable, Storable, FieldRepr)]
#[storable(base)]
#[tag = "dust-initial-nonce[v1]"]
pub struct InitialNonce(pub HashOutput);
tag_enforcement_test!(InitialNonce);

pub fn initial_nonce(output_no: u32, intent_hash: IntentHash) -> InitialNonce {
    InitialNonce(persistent_commit(&output_no, intent_hash.0))
}

impl UtxoSpend {
    pub fn initial_nonce(&self) -> InitialNonce {
        initial_nonce(self.output_no, self.intent_hash)
    }
}

impl Utxo {
    pub fn initial_nonce(&self) -> InitialNonce {
        initial_nonce(self.output_no, self.intent_hash)
    }
}

pub fn dust_nonce(initial_nonce: &InitialNonce, seq: u32, sk: &DustSecretKey) -> Fr {
    transient_hash((*initial_nonce, seq, sk.0).field_vec().as_ref())
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serializable, Storable, FieldRepr)]
#[storable(base)]
#[tag = "dust-output[v1]"]
pub struct DustOutput {
    pub initial_value: u128,
    pub owner: DustPublicKey,
    pub nonce: Fr,
    pub seq: u32,
    pub ctime: Timestamp,
}
tag_enforcement_test!(DustOutput);

impl DustOutput {
    pub fn updated_value(
        &self,
        gen_info: &DustGenerationInfo,
        now: Timestamp,
        params: &DustParameters,
    ) -> u128 {
        // There are up to four linear segments:
        // 1. Generating (from inp.ctime to tfull, the time dust fills to the cap)
        // 2. Constant full (from tfull to gen.dtime)
        // 3. Decaying (from gen.dtime to tempty, the time dust reaches zero)
        // 4. Constant empty (from tempty onwards)
        //
        // If gen.dtime <= tfull, phase 2 doesn't occur.
        // If gen.dtime <= inp.ctime Phases 1 and 2 do not occur.
        //
        // The maximum capacity is gen.value * night_dust_ratio.
        let vfull = gen_info
            .value
            .saturating_mul(params.night_dust_ratio as u128);
        // The slope of generation and decay for a specific dust UTXO
        // is proportional to the value of its backing night.
        let rate = gen_info
            .value
            .saturating_mul(params.generation_decay_rate as u128);
        // Note that we aren't constraining the end to be after the start, instead
        // we're clamping the output to the reasonable region of outputs.
        let tstart_phase_1 = self.ctime;
        let tend_phase_12 = Timestamp::min(gen_info.dtime, now);
        let dt_phase_12 = (tend_phase_12 - tstart_phase_1).as_seconds();
        let dt_phase_12 = if dt_phase_12 >= 0 {
            dt_phase_12 as u128
        } else {
            0
        };
        let value_phase_1_unchecked = dt_phase_12
            .saturating_mul(rate)
            .saturating_add(self.initial_value);
        let value_phase_12 = u128::min(value_phase_1_unchecked, vfull);
        // Again, we aren't constraining the end to be after the start, instead
        // we're clamping the output to the reasonable region of outputs.
        let tstart_phase_3 = gen_info.dtime;
        let tend_phase_3 = now;
        let dt_phase_3 = (tend_phase_3 - tstart_phase_3).as_seconds();
        let dt_phase_3 = if dt_phase_3 >= 0 {
            dt_phase_3 as u128
        } else {
            0
        };
        value_phase_12.saturating_sub(dt_phase_3.saturating_mul(rate))
    }

    pub fn commitment(&self) -> DustCommitment {
        DustPreProjection {
            initial_value: self.initial_value,
            owner: self.owner,
            nonce: self.nonce,
            ctime: self.ctime,
        }
        .commitment()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serializable, Storable)]
#[storable(base)]
#[tag = "qualified-dust-output[v1]"]
pub struct QualifiedDustOutput {
    pub initial_value: u128,
    pub owner: DustPublicKey,
    pub nonce: Fr,
    pub seq: u32,
    pub ctime: Timestamp,
    pub backing_night: InitialNonce,
    pub mt_index: u64,
}
tag_enforcement_test!(QualifiedDustOutput);

impl QualifiedDustOutput {
    pub fn commitment(&self) -> DustCommitment {
        DustPreProjection {
            initial_value: self.initial_value,
            owner: self.owner,
            nonce: self.nonce,
            ctime: self.ctime,
        }
        .commitment()
    }

    pub fn nullifier(&self, sk: &DustSecretKey) -> DustNullifier {
        DustPreProjection {
            initial_value: self.initial_value,
            owner: sk.clone(),
            nonce: self.nonce,
            ctime: self.ctime,
        }
        .nullifier()
    }
}

impl From<QualifiedDustOutput> for DustOutput {
    fn from(value: QualifiedDustOutput) -> Self {
        DustOutput {
            initial_value: value.initial_value,
            owner: value.owner,
            nonce: value.nonce,
            seq: value.seq,
            ctime: value.ctime,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serializable, Storable, FieldRepr)]
#[storable(base)]
#[tag = "dust-pre-projection[v1]"]
pub struct DustPreProjection<T: Serializable + Deserializable + Clone + Sync + Send + 'static> {
    pub initial_value: u128,
    pub owner: T,
    pub nonce: Fr,
    pub ctime: Timestamp,
}
tag_enforcement_test!(DustPreProjection<DustPublicKey>);

impl DustPreProjection<DustPublicKey> {
    pub fn commitment(&self) -> DustCommitment {
        DustCommitment(transient_commit(
            &self.field_vec(),
            b"mdn:dust:cm".field_vec()[0],
        ))
    }
}

impl DustPreProjection<DustSecretKey> {
    pub fn nullifier(&self) -> DustNullifier {
        DustNullifier(transient_commit(
            &self.field_vec(),
            b"mdn:dust:nul".field_vec()[0],
        ))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serializable, Storable, FieldRepr)]
#[storable(base)]
#[tag = "dust-generation-info[v1]"]
pub struct DustGenerationInfo {
    pub value: u128,
    pub owner: DustPublicKey,
    pub nonce: InitialNonce,
    pub dtime: Timestamp,
}
tag_enforcement_test!(DustGenerationInfo);

pub(crate) const DUST_GENERATION_INFO_SIZE: usize = 16 + FR_BYTES + PERSISTENT_HASH_BYTES + 8;

impl DustGenerationInfo {
    pub fn merkle_hash(&self) -> HashOutput {
        upgrade_from_transient(transient_hash(&self.field_vec()[..]))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serializable, Storable)]
#[storable(base)]
#[tag = "dust-generation-uniqueness-info"]
pub struct DustGenerationUniquenessInfo {
    pub value: u128,
    pub owner: DustPublicKey,
    pub nonce: InitialNonce,
}
tag_enforcement_test!(DustGenerationUniquenessInfo);

impl From<DustGenerationInfo> for DustGenerationUniquenessInfo {
    fn from(info: DustGenerationInfo) -> DustGenerationUniquenessInfo {
        DustGenerationUniquenessInfo {
            value: info.value,
            owner: info.owner,
            nonce: info.nonce,
        }
    }
}

#[derive(Storable)]
#[derive_where(Clone, PartialEq, Eq; P)]
#[storable(db = D)]
#[tag = "dust-spend[v1]"]
pub struct DustSpend<P: ProofKind<D>, D: DB> {
    pub v_fee: u128,
    pub old_nullifier: DustNullifier,
    pub new_commitment: DustCommitment,
    pub proof: P::LatestProof,
}
tag_enforcement_test!(DustSpend<(), InMemoryDB>);

impl<P: ProofKind<D>, D: DB> Debug for DustSpend<P, D> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("DustSpend")
            .field("v_fee", &self.v_fee)
            .field("old_nullifier", &Symbol("<dust nullifier>"))
            .field("new_commitment", &Symbol("<dust commitment>"))
            .field("proof", &Symbol("<proof>"))
            .finish()
    }
}

impl<P: ProofKind<D>, D: DB> FieldRepr for DustSpend<P, D> {
    fn field_repr<W: MemWrite<Fr>>(&self, writer: &mut W) {
        self.v_fee.field_repr(writer);
        self.old_nullifier.field_repr(writer);
        self.new_commitment.field_repr(writer);
    }
    fn field_size(&self) -> usize {
        self.v_fee.field_size() + self.old_nullifier.field_size() + self.new_commitment.field_size()
    }
}

impl<P: ProofKind<D>, D: DB> Aligned for DustSpend<P, D> {
    fn alignment() -> Alignment {
        Alignment(vec![
            AlignmentSegment::Atom(AlignmentAtom::Bytes { length: 16 }),
            AlignmentSegment::Atom(AlignmentAtom::Field),
            AlignmentSegment::Atom(AlignmentAtom::Field),
        ])
    }
}

impl<P: ProofKind<D>, D: DB> From<DustSpend<P, D>> for Value {
    fn from(spend: DustSpend<P, D>) -> Value {
        Value(vec![
            spend.v_fee.into(),
            spend.old_nullifier.0.into(),
            spend.new_commitment.0.into(),
        ])
    }
}

impl<D: DB> DustSpend<ProofPreimageMarker, D> {
    async fn prove(
        &self,
        prover: impl ProvingProvider,
        segment_id: u16,
        binding: Pedersen,
    ) -> Result<DustSpend<ProofMarker, D>, ProvingError> {
        let proof = prover
            .prove(
                &self.proof,
                Some(transient_hash(
                    &(
                        Fr::from_le_bytes(b"midnight:dust:proof"),
                        segment_id,
                        binding,
                    )
                        .field_vec(),
                )),
            )
            .await?;
        Ok(DustSpend {
            v_fee: self.v_fee,
            old_nullifier: self.old_nullifier,
            new_commitment: self.new_commitment,
            proof,
        })
    }
}

impl<P: ProofKind<D>, D: DB> DustSpend<P, D> {
    pub(crate) fn erase_proofs(&self) -> DustSpend<(), D> {
        DustSpend {
            v_fee: self.v_fee,
            old_nullifier: self.old_nullifier,
            new_commitment: self.new_commitment,
            proof: (),
        }
    }

    #[cfg(not(feature = "proof-verifying"))]
    pub(crate) fn well_formed(
        &self,
        _state_ref: &impl StateReference<D>,
        _segment_id: u16,
        _binding: Pedersen,
        _strictness: WellFormedStrictness,
        _ctime: Timestamp,
    ) -> Result<(), MalformedTransaction<D>> {
        Ok(())
    }

    #[cfg(feature = "proof-verifying")]
    pub(crate) fn well_formed(
        &self,
        state_ref: &impl StateReference<D>,
        segment_id: u16,
        binding: Pedersen,
        strictness: WellFormedStrictness,
        ctime: Timestamp,
    ) -> Result<(), MalformedTransaction<D>> {
        if strictness.verify_native_proofs {
            state_ref.dust_spend_check(ctime, |params, commitment_root, generation_root| {
                let mut prog = Vec::new();
                // Check commitment merkle tree root
                prog.extend::<[Op<ResultModeGather, D>; 6]>(HistoricMerkleTree_check_root!(
                    [Key::Value(0u8.into())],
                    false,
                    32,
                    Fr,
                    commitment_root.0
                ));
                // Check generation merkle tree root
                prog.extend(HistoricMerkleTree_check_root!(
                    [Key::Value(1u8.into())],
                    false,
                    32,
                    Fr,
                    generation_root.0
                ));
                // Read spend
                prog.extend(Cell_read!([Key::Value(5u8.into())], false, DustSpend));
                // Read ctime
                prog.extend(Cell_read!([Key::Value(4u8.into())], false, u64));
                // Read dust parameters
                prog.extend(Cell_read!([Key::Value(3u8.into())], false, DustParameters));
                // Insert old nullifier
                prog.extend(Set_insert!(
                    [Key::Value(2u8.into())],
                    false,
                    Fr,
                    self.old_nullifier.0
                ));
                // Read ctime
                prog.extend(Cell_read!([Key::Value(4u8.into())], false, u64));
                // Insert new commitment
                prog.extend(HistoricMerkleTree_insert_hash!(
                    [Key::Value(0u8.into())],
                    false,
                    32,
                    Fr,
                    HashOutput::from(self.new_commitment)
                ));

                let mut pis = vec![];
                pis.push(transient_hash(
                    &(
                        Fr::from_le_bytes(b"midnight:dust:proof"),
                        segment_id,
                        binding,
                    )
                        .field_vec(),
                ));
                for op in with_outputs(
                    prog.into_iter(),
                    [
                        true.into(),         // commitment root check
                        true.into(),         // nullifier root check
                        self.clone().into(), // dust spend read
                        ctime.into(),        // ctime read
                        params.into(),       // parameter read
                        ctime.into(),        // ctime read
                    ]
                    .into_iter(),
                ) {
                    op.field_repr(&mut pis);
                }
                debug_assert_eq!(pis.len(), DUST_SPEND_PIS);
                P::latest_proof_verify(
                    &SPEND_VK,
                    &self.proof,
                    pis,
                    strictness.proof_verification_mode,
                )
                .map_err(|_| MalformedTransaction::InvalidDustSpendProof {
                    declared_time: ctime,
                    dust_spend: Box::new(self.erase_proofs()),
                })
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Storable)]
#[derive_where(Clone, PartialEq, Eq, Debug; S)]
#[storable(db = D)]
#[tag = "dust-registration[v1]"]
pub struct DustRegistration<S: SignatureKind<D>, D: DB> {
    pub night_key: VerifyingKey,
    pub dust_address: Option<Sp<DustPublicKey, D>>,
    pub allow_fee_payment: u128,
    #[allow(clippy::type_complexity)]
    pub signature: Option<Sp<S::Signature<(u16, ErasedIntent<D>)>, D>>,
}
tag_enforcement_test!(DustRegistration<(), InMemoryDB>);

impl<S: SignatureKind<D>, D: DB> DustRegistration<S, D> {
    pub(crate) fn erase_signatures(&self) -> DustRegistration<(), D> {
        DustRegistration {
            night_key: self.night_key.clone(),
            dust_address: self.dust_address.clone(),
            allow_fee_payment: self.allow_fee_payment,
            signature: self.signature.as_ref().map(|_| Sp::new(())),
        }
    }

    pub(crate) fn well_formed(
        &self,
        segment_id: u16,
        parent: &ErasedIntent<D>,
        strictness: WellFormedStrictness,
    ) -> Result<(), MalformedTransaction<D>> {
        if !strictness.verify_signatures
            || self.signature.as_ref().map(|sig| {
                S::signature_verify(
                    &parent.data_to_sign(segment_id),
                    self.night_key.clone(),
                    sig,
                )
            }) == Some(true)
        {
            Ok(())
        } else {
            warn!(registration = ?self, "signature verification of dust registration failed");
            Err(MalformedTransaction::InvalidDustRegistrationSignature {
                registration: Box::new(self.erase_signatures()),
            })
        }
    }
}

#[derive(Storable)]
#[derive_where(Debug)]
#[derive_where(Clone, PartialEq, Eq; S, P)]
#[storable(db = D)]
#[tag = "dust-actions[v1]"]
pub struct DustActions<S: SignatureKind<D>, P: ProofKind<D>, D: DB> {
    pub spends: storage::storage::Array<DustSpend<P, D>, D>,
    pub registrations: storage::storage::Array<DustRegistration<S, D>, D>,
    pub ctime: Timestamp,
}
tag_enforcement_test!(DustActions<(), (), InMemoryDB>);

impl<S: SignatureKind<D>, D: DB> DustActions<S, ProofPreimageMarker, D> {
    pub(crate) async fn prove(
        &self,
        mut prover: impl ProvingProvider,
        segment_id: u16,
        binding: Pedersen,
    ) -> Result<DustActions<S, ProofMarker, D>, ProvingError> {
        let spends = join_all(
            self.spends
                .iter_deref()
                .map(|spend| spend.prove(prover.split(), segment_id, binding)),
        )
        .await;
        Ok(DustActions {
            spends: spends.into_iter().collect::<Result<_, _>>()?,
            registrations: self.registrations.clone(),
            ctime: self.ctime,
        })
    }
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> DustActions<S, P, D> {
    pub(crate) fn erase_proofs(&self) -> DustActions<S, (), D> {
        DustActions {
            spends: self
                .spends
                .iter_deref()
                .map(DustSpend::erase_proofs)
                .collect(),
            registrations: self.registrations.clone(),
            ctime: self.ctime,
        }
    }

    pub(crate) fn erase_signatures(&self) -> DustActions<(), P, D> {
        DustActions {
            spends: self.spends.clone(),
            registrations: self
                .registrations
                .iter_deref()
                .map(DustRegistration::erase_signatures)
                .collect(),
            ctime: self.ctime,
        }
    }

    pub(crate) fn well_formed(
        &self,
        ref_state: &impl StateReference<D>,
        strictness: WellFormedStrictness,
        segment_id: u16,
        parent: &ErasedIntent<D>,
        tblock: Timestamp,
    ) -> Result<(), MalformedTransaction<D>> {
        let binding = parent.binding_commitment;
        if self.spends.is_empty() && self.registrations.is_empty() {
            warn!("non-canonical dust actions: empty");
            return Err(MalformedTransaction::NotNormalized);
        }
        for spend in self.spends.iter() {
            spend.well_formed(ref_state, segment_id, binding, strictness, self.ctime)?;
        }
        for reg in self.registrations.iter() {
            ref_state.stateless_check(|| reg.well_formed(segment_id, parent, strictness))?;
        }
        ref_state.param_check(true, |params| if self.ctime > tblock || self.ctime + params.dust.dust_grace_period < tblock {
            warn!(ctime = ?self.ctime, ?tblock, grace_period = ?params.dust.dust_grace_period, "dust actions out of TTL range");
            Err(MalformedTransaction::OutOfDustValidityWindow {
                dust_ctime: self.ctime,
                // NOTE: our subtraction impl is saturating, that's fine here.
                validity_start: tblock - params.dust.dust_grace_period,
                validity_end: tblock,
            })
        } else {
            Ok(())
        })?;
        ref_state.stateless_check(|| {
            // Make sure that we are not registering for the same night key more than once
            let mut night_keys = self.registrations.iter_deref().map(|reg| &reg.night_key).collect::<Vec<_>>();
            night_keys.sort();
            if let Some(window) = night_keys.windows(2).find(|window| window[0] == window[1]) {
                warn!(key = ?window[0], "non-canonical dust actions: multiple registrations for same key");
                return Err(MalformedTransaction::MultipleDustRegistrationsForKey {
                    key: window[0].clone(),
                });
            }
            Ok(())
        })?;
        // Make sure that each registration has sufficient unclaimed night in the ref utxo state to
        // cover its allowed fee payment field.
        if strictness.enforce_balancing {
            self.registrations
                .iter()
                .filter(|reg| reg.allow_fee_payment > 0)
                .try_for_each(|reg| {
                    ref_state.generationless_fee_availability_check(
                        parent,
                        &reg.night_key,
                        |available| {
                            if available < reg.allow_fee_payment {
                                warn!(
                                    ?available,
                                    ?reg,
                                    "insufficient fees to cover registration fee allowance"
                                );
                                Err(MalformedTransaction::InsufficientDustForRegistrationFee {
                                    registration: Box::new(reg.erase_signatures()),
                                    available_dust: available,
                                })
                            } else {
                                Ok(())
                            }
                        },
                    )
                })?;
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serializable, Storable, Serialize, Deserialize)]
#[storable(base)]
#[tag = "dust-parameters[v1]"]
pub struct DustParameters {
    pub night_dust_ratio: u64,
    pub generation_decay_rate: u32,
    pub dust_grace_period: Duration,
}
tag_enforcement_test!(DustParameters);

impl DustParameters {
    pub const fn time_to_cap(&self) -> Duration {
        Duration::from_secs(
            self.night_dust_ratio
                .div_ceil(self.generation_decay_rate as u64) as i128,
        )
    }
}

impl FieldRepr for DustParameters {
    fn field_repr<W: MemWrite<Fr>>(&self, writer: &mut W) {
        let grace_period_secs = self.dust_grace_period.as_seconds();
        let grace_period_u32 = i128::clamp(grace_period_secs, 0, u32::MAX as i128) as u32;
        self.night_dust_ratio.field_repr(writer);
        self.generation_decay_rate.field_repr(writer);
        grace_period_u32.field_repr(writer);
    }
    fn field_size(&self) -> usize {
        3
    }
}

impl Aligned for DustParameters {
    fn alignment() -> Alignment {
        Alignment(vec![
            AlignmentSegment::Atom(AlignmentAtom::Bytes { length: 8 }),
            AlignmentSegment::Atom(AlignmentAtom::Bytes { length: 4 }),
            AlignmentSegment::Atom(AlignmentAtom::Bytes { length: 4 }),
        ])
    }
}

impl From<DustParameters> for Value {
    fn from(params: DustParameters) -> Value {
        let grace_period_secs = params.dust_grace_period.as_seconds();
        let grace_period_u32 = i128::clamp(grace_period_secs, 0, u32::MAX as i128) as u32;
        Value(vec![
            params.night_dust_ratio.into(),
            params.generation_decay_rate.into(),
            grace_period_u32.into(),
        ])
    }
}

#[derive(Storable)]
#[derive_where(Clone, Debug, PartialEq, Eq)]
#[storable(db = D)]
#[tag = "dust-utxo-state[v1]"]
#[must_use]
pub struct DustUtxoState<D: DB> {
    pub commitments: MerkleTree<(), D>,
    pub commitments_first_free: u64,
    pub nullifiers: HashSet<DustNullifier, D>,
    pub root_history: TimeFilterMap<Identity<MerkleTreeDigest>, D>,
}
tag_enforcement_test!(DustUtxoState<InMemoryDB>);

impl<D: DB> Default for DustUtxoState<D> {
    fn default() -> Self {
        DustUtxoState {
            commitments: MerkleTree::blank(DUST_COMMITMENT_TREE_DEPTH),
            commitments_first_free: 0,
            nullifiers: HashSet::default(),
            root_history: TimeFilterMap::new(),
        }
    }
}

#[derive(Storable)]
#[derive_where(Clone, Debug, PartialEq, Eq)]
#[storable(db = D)]
#[tag = "dust-generation-state[v1]"]
#[must_use]
pub struct DustGenerationState<D: DB> {
    pub address_delegation: Map<UserAddress, DustPublicKey, D>,
    pub generating_tree: MerkleTree<DustGenerationInfo, D>,
    pub generating_tree_first_free: u64,
    pub generating_set: HashSet<DustGenerationUniquenessInfo, D>,
    pub night_indices: HashMap<InitialNonce, u64, D>,
    pub root_history: TimeFilterMap<Identity<MerkleTreeDigest>, D>,
}
tag_enforcement_test!(DustGenerationState<InMemoryDB>);

impl<D: DB> Default for DustGenerationState<D> {
    fn default() -> Self {
        DustGenerationState {
            address_delegation: Map::default(),
            generating_tree: MerkleTree::blank(DUST_GENERATION_TREE_DEPTH),
            generating_tree_first_free: 0,
            generating_set: HashSet::default(),
            night_indices: HashMap::default(),
            root_history: TimeFilterMap::new(),
        }
    }
}

#[derive(Storable)]
#[derive_where(Clone, Debug, PartialEq, Eq, Default)]
#[storable(db = D)]
#[tag = "dust-state[v1]"]
#[must_use]
pub struct DustState<D: DB> {
    pub utxo: DustUtxoState<D>,
    pub generation: DustGenerationState<D>,
}
tag_enforcement_test!(DustState<InMemoryDB>);

impl<D: DB> DustState<D> {
    pub(crate) fn apply_spend<P: ProofKind<D>>(
        &self,
        spend: &DustSpend<P, D>,
        time: Timestamp,
        context: &TransactionContext<D>,
        _params: &DustParameters,
        mut event_push: impl FnMut(EventDetails<D>),
    ) -> Result<Self, TransactionInvalid<D>> {
        let mut state = self.clone();
        if state.utxo.nullifiers.member(&spend.old_nullifier) {
            warn!(?spend.old_nullifier, "dust double spend");
            return Err(TransactionInvalid::DustDoubleSpend(spend.old_nullifier));
        }
        state.utxo.nullifiers = state.utxo.nullifiers.insert(spend.old_nullifier);
        state.utxo.commitments = state
            .utxo
            .commitments
            .try_update_hash(
                self.utxo.commitments_first_free,
                spend.new_commitment.into(),
                (),
            )
            .map_err(TransactionInvalid::MerkleTreeError)?;
        state.utxo.commitments_first_free += 1;
        event_push(EventDetails::DustSpendProcessed {
            commitment: spend.new_commitment,
            commitment_index: &state.utxo.commitments_first_free - 1,
            nullifier: spend.old_nullifier,
            v_fee: spend.v_fee,
            declared_time: time,
            block_time: context.block_context.tblock,
        });
        Ok(state)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_registration<S: SignatureKind<D>>(
        &self,
        utxo: &UtxoState<D>,
        mut fees_remaining: u128,
        parent_intent: &ErasedIntent<D>,
        registration: &DustRegistration<S, D>,
        dust_params: &DustParameters,
        tnow: Timestamp,
        context: &TransactionContext<D>,
        mut event_push: impl FnMut(EventDetails<D>),
    ) -> Result<(Self, u128), TransactionInvalid<D>> {
        let night_address = UserAddress::from(registration.night_key.clone());
        let mut state = self.clone();
        match registration.dust_address.as_ref() {
            None => {
                if !state
                    .generation
                    .address_delegation
                    .contains_key(&night_address)
                {
                    warn!(?night_address, "non-registered deregistration attempt");
                    return Err(TransactionInvalid::DustDeregistrationNotRegistered(
                        night_address,
                    ));
                }
                state.generation.address_delegation =
                    state.generation.address_delegation.remove(&night_address);
            }
            Some(dust_address) => {
                state.generation.address_delegation = state
                    .generation
                    .address_delegation
                    .insert(night_address, **dust_address);
            }
        }
        let owned_outputs = parent_intent
            .guaranteed_unshielded_offer
            .iter()
            .flat_map(|o| o.outputs.iter_deref().enumerate())
            .filter(|(_, o)| o.owner == night_address && o.type_ == NIGHT)
            .collect::<Vec<_>>();
        let dust_in = self.generationless_fee_availability(
            utxo,
            parent_intent,
            &registration.night_key,
            dust_params,
        );
        let fee_paid = u128::min(
            fees_remaining,
            u128::min(registration.allow_fee_payment, dust_in),
        );
        fees_remaining -= fee_paid; // subtraction safe due to `min` above
        let dust_out = dust_in - fee_paid;
        if let Some(dust_addr) = registration.dust_address.as_ref() {
            let output_sum = owned_outputs
                .iter()
                .map(|(_, o)| o.value)
                .fold(0, u128::saturating_add);
            for (output_no, output) in owned_outputs.into_iter() {
                // NOTE: The ratio calculation could overflow even u128. As a
                // result, we quantize it.
                const DISTRIBUTION_RESOLUTION: u128 = 10_000;
                // NOTE: This arithmetic *should* be safe because of the NIGHT / DUST caps, and the
                // resolution, but we use saturating arithmetic anyway just in case.
                // NOTE: The corner case of output_sum = 0 can occur, because we currently don't
                // explicitly reject zero-value outputs. In this case, we *used to* panic here,
                // which the node would unwind by catching the panic, and rejecting the
                // transaction.
                // Instead, we now mark the transaction invalid. As this also results in
                // transaction rejection, this is a *backwards-compatible* fix.
                let ratio = output
                    .value
                    .saturating_mul(DISTRIBUTION_RESOLUTION)
                    .checked_div(output_sum)
                    .ok_or(TransactionInvalid::DivideByZero)?;
                let initial_value = ratio.saturating_mul(dust_out) / DISTRIBUTION_RESOLUTION;
                state = state.fresh_dust_output(
                    initial_nonce(output_no as u32, parent_intent.intent_hash(0)),
                    initial_value,
                    output.value,
                    **dust_addr,
                    tnow,
                    context.block_context.tblock,
                    &mut event_push,
                )?;
            }
        }
        Ok((state, fees_remaining))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn fresh_dust_output(
        &self,
        initial_nonce: InitialNonce,
        initial_value: u128,
        night_value: u128,
        dust_addr: DustPublicKey,
        tnow: Timestamp,
        tblock: Timestamp,
        mut event_push: impl FnMut(EventDetails<D>),
    ) -> Result<Self, DustStateError> {
        let mut state = self.clone();
        let seq = 0u32;
        let dust_pre_projection = DustPreProjection {
            initial_value,
            owner: dust_addr,
            nonce: transient_hash((initial_nonce, seq, dust_addr).field_vec().as_ref()),
            ctime: tnow,
        };
        let dust_commitment = dust_pre_projection.commitment();
        state.utxo.commitments = state
            .utxo
            .commitments
            .try_update_hash(
                state.utxo.commitments_first_free,
                dust_commitment.into(),
                (),
            )
            .map_err(DustStateError::MerkleTreeError)?;
        state.utxo.commitments_first_free += 1;
        let gen_info = DustGenerationInfo {
            value: night_value,
            owner: dust_addr,
            nonce: initial_nonce,
            dtime: Timestamp::MAX,
        };
        if self.generation.generating_set.member(&gen_info.into()) {
            warn!(?gen_info, "already present generation info");
            return Err(DustStateError::GenerationInfoAlreadyPresent(gen_info));
        }
        state.generation.generating_set = state.generation.generating_set.insert(gen_info.into());
        state.generation.generating_tree = state
            .generation
            .generating_tree
            .try_update_hash(
                state.generation.generating_tree_first_free,
                gen_info.merkle_hash(),
                gen_info,
            )
            .map_err(DustStateError::MerkleTreeError)?;
        state.generation.night_indices = state
            .generation
            .night_indices
            .insert(initial_nonce, self.generation.generating_tree_first_free);
        state.generation.generating_tree_first_free += 1;

        event_push(EventDetails::DustInitialUtxo {
            output: QualifiedDustOutput {
                initial_value,
                owner: dust_addr,
                nonce: dust_pre_projection.nonce,
                seq: 0,
                ctime: tnow,
                backing_night: initial_nonce,
                mt_index: state.utxo.commitments_first_free - 1,
            },
            generation: gen_info,
            generation_index: state.generation.generating_tree_first_free - 1,
            block_time: tblock,
        });

        Ok(state)
    }

    pub(crate) fn generationless_fee_availability(
        &self,
        utxo_state: &UtxoState<D>,
        parent_intent: &ErasedIntent<D>,
        night_key: &VerifyingKey,
        params: &DustParameters,
    ) -> u128 {
        let generationless_inputs = parent_intent
            .guaranteed_unshielded_offer
            .iter()
            .flat_map(|o| o.inputs.iter_deref())
            .filter(|i| &i.owner == night_key && i.type_ == NIGHT)
            .filter(|i| {
                !self
                    .generation
                    .night_indices
                    .contains_key(&i.initial_nonce())
            });
        let Some(tend) = parent_intent.dust_actions.as_ref().map(|da| da.ctime) else {
            error!("reached generationless_fee_availability without dust actions");
            return 0;
        };
        generationless_inputs
            .map(|i| {
                let vfull = i.value.saturating_mul(params.night_dust_ratio as u128);
                let rate = i.value.saturating_mul(params.generation_decay_rate as u128);
                let Some(tstart) = utxo_state
                    .utxos
                    .get(&Utxo::from(i.clone()))
                    .map(|meta| meta.ctime)
                else {
                    warn!("couldn't get metadata of input utxo, it's likely invalid");
                    return 0;
                };
                let dt = (tend - tstart).as_seconds();
                let dt = if dt < 0 { 0 } else { dt as u128 };
                let value_unchecked = dt.saturating_mul(rate);
                u128::clamp(value_unchecked, 0, vfull)
            })
            .fold(0u128, |a, b| a.saturating_add(b))
    }

    pub(crate) fn apply_offer<S: SignatureKind<D>>(
        &self,
        offer: &UnshieldedOffer<S, D>,
        parent: &ErasedIntent<D>,
        segment: u16,
        context: &TransactionContext<D>,
        mut event_push: impl FnMut(EventDetails<D>),
    ) -> Result<Self, TransactionInvalid<D>> {
        let mut state = self.clone();
        for input in offer.inputs.iter_deref().filter(|i| i.type_ == NIGHT) {
            let Some(idx) = state.generation.night_indices.get(&input.initial_nonce()) else {
                continue;
            };
            let Some(mut gen_info) = state
                .generation
                .generating_tree
                .index(*idx)
                .map(|gen_info| *gen_info.1)
            else {
                error!(utxo = ?Utxo::from(input.clone()), ?idx, "invariant violated: `night_indices` reference not backed in `generating_tree`");
                debug_assert!(false);
                continue;
            };
            gen_info.dtime = context.block_context.tblock;
            // TODO: We maybe can do better than immediately rehashing here... But not much,
            // because anything in the insertion evidence in the event *will* need to be computed
            // here.
            state.generation.generating_tree = state
                .generation
                .generating_tree
                .try_update_hash(*idx, gen_info.merkle_hash(), gen_info)
                .map_err(TransactionInvalid::MerkleTreeError)?
                .rehash();
            event_push(EventDetails::DustGenerationDtimeUpdate {
                update: state
                    .generation
                    .generating_tree
                    .insertion_evidence(*idx)
                    .expect("must be able to produce evidence for updated path"),
                block_time: context.block_context.tblock,
            });
        }
        for (output_no, output) in offer
            .outputs
            .iter_deref()
            .enumerate()
            .filter(|(_, o)| o.type_ == NIGHT)
        {
            let Some(dust_addr) = state.generation.address_delegation.get(&output.owner) else {
                continue;
            };
            let handled_by_registration = segment == 0
                && parent
                    .dust_actions
                    .iter()
                    .flat_map(|actions| actions.registrations.iter_deref())
                    .any(|reg| output.owner == reg.night_key.clone().into());
            if !handled_by_registration {
                state = state.fresh_dust_output(
                    initial_nonce(output_no as u32, parent.intent_hash(segment)),
                    0,
                    output.value,
                    *dust_addr,
                    context.block_context.tblock,
                    context.block_context.tblock,
                    &mut event_push,
                )?;
            }
        }
        Ok(state)
    }

    pub(crate) fn post_block_update(&self, tblock: Timestamp, global_ttl: Duration) -> Self {
        let mut res = self.clone();
        res.utxo.commitments = res.utxo.commitments.rehash();
        res.utxo.root_history = self
            .utxo
            .root_history
            .insert(
                tblock,
                res.utxo
                    .commitments
                    .root()
                    .expect("rehashed tree should have root"),
            )
            .filter(tblock - global_ttl);
        res.generation.generating_tree = res.generation.generating_tree.rehash();
        res.generation.root_history = self
            .generation
            .root_history
            .insert(
                tblock,
                res.generation
                    .generating_tree
                    .root()
                    .expect("rehashed tree should have root"),
            )
            .filter(tblock - global_ttl);
        res
    }
}

pub const INITIAL_DUST_PARAMETERS: DustParameters = DustParameters {
    night_dust_ratio: 5 * (SPECKS_PER_DUST / STARS_PER_NIGHT) as u64, // 5 DUST per NIGHT
    generation_decay_rate: 8_267, // Works out to a generation time of approximately 1 week.
    dust_grace_period: Duration::from_hours(3),
};

#[derive(Copy, Clone, Debug, Serializable, Storable)]
#[storable(base)]
#[tag = "dust-wallet-utxo-state[v1]"]
pub struct DustWalletUtxoState {
    utxo: QualifiedDustOutput,
    pending_until: Option<Timestamp>,
}
tag_enforcement_test!(DustWalletUtxoState);

#[derive(Debug)]
pub enum DustSpendError {
    BackingNightNotFound(Box<QualifiedDustOutput>),
    NotEnoughDust { available: u128, required: u128 },
    DustUtxoNotTracked(Box<QualifiedDustOutput>),
    MerkleTreeNotRehashed(&'static str),
}

impl Display for DustSpendError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use DustSpendError as E;
        match self {
            E::BackingNightNotFound(utxo) => {
                write!(f, "backing Night of Dust UTXO not found (utxo: {utxo:?})")
            }
            E::NotEnoughDust {
                available,
                required,
            } => write!(
                f,
                "attempted to spend {required} Specks of Dust, but only {available} are available from this UTXO"
            ),
            E::DustUtxoNotTracked(utxo) => write!(
                f,
                "attempted to spend Dust UTXO that's not in the wallet state: {utxo:?}"
            ),
            E::MerkleTreeNotRehashed(tree) => write!(f, "{tree} Merkle tree is not fully rehashed"),
        }
    }
}

impl Error for DustSpendError {}

#[derive(Debug, Storable)]
#[derive_where(Clone)]
#[storable(db = D)]
#[tag = "dust-local-state[v1]"]
pub struct DustLocalState<D: DB> {
    pub generating_tree: MerkleTree<DustGenerationInfo, D>,
    generating_tree_first_free: u64,
    pub commitment_tree: MerkleTree<(), D>,
    commitment_tree_first_free: u64,
    night_indices: HashMap<InitialNonce, u64, D>,
    dust_utxos: HashMap<DustNullifier, DustWalletUtxoState, D>,
    pub sync_time: Timestamp,
    pub params: DustParameters,
}
tag_enforcement_test!(DustLocalState<InMemoryDB>);

#[derive(Clone)]
pub struct DustStateChanges {
    pub received_utxos: Vec<QualifiedDustOutput>,
    pub spent_utxos: Vec<QualifiedDustOutput>,
    pub source: TransactionHash,
}

impl DustStateChanges {
    pub fn can_merge(&self, other: &DustStateChanges) -> bool {
        self.source == other.source
    }

    pub fn merge(&mut self, other: DustStateChanges) {
        self.received_utxos.extend(other.received_utxos);
        self.spent_utxos.extend(other.spent_utxos);
    }
}

pub struct WithDustStateChanges<T> {
    pub changes: Vec<DustStateChanges>,
    pub result: T,
}

impl<T> WithDustStateChanges<T> {
    pub fn new(result: T) -> WithDustStateChanges<T> {
        WithDustStateChanges {
            changes: Vec::new(),
            result,
        }
    }
}

impl<T> WithDustStateChanges<T> {
    pub fn add_change(mut self, change: DustStateChanges) -> Self {
        if let Some(last_change) = self.changes.last_mut() {
            if last_change.can_merge(&change) {
                last_change.merge(change);
            } else {
                self.changes.push(change);
            }
        } else {
            self.changes.push(change);
        }

        WithDustStateChanges {
            changes: self.changes,
            result: self.result,
        }
    }

    pub fn maybe_add_change(self, maybe_change: Option<DustStateChanges>) -> Self {
        match maybe_change {
            Some(change) => self.add_change(change),
            None => self,
        }
    }

    pub fn with_result(self, result: T) -> Self {
        WithDustStateChanges {
            changes: self.changes,
            result,
        }
    }
}

impl<D: DB> DustLocalState<D> {
    pub fn new(params: DustParameters) -> Self {
        DustLocalState {
            generating_tree: MerkleTree::blank(DUST_GENERATION_TREE_DEPTH),
            generating_tree_first_free: 0,
            commitment_tree: MerkleTree::blank(DUST_COMMITMENT_TREE_DEPTH),
            commitment_tree_first_free: 0,
            night_indices: HashMap::new(),
            dust_utxos: HashMap::new(),
            sync_time: Timestamp::default(),
            params,
        }
    }

    pub fn wallet_balance(&self, time: Timestamp) -> u128 {
        self.utxos()
            .filter_map(|utxo| {
                let gen_idx = *self.night_indices.get(&utxo.backing_night)?;
                let gen_info = self.generating_tree.index(gen_idx)?.1;
                Some(DustOutput::from(utxo).updated_value(gen_info, time, &self.params))
            })
            .sum()
    }

    pub fn utxos(&self) -> impl Iterator<Item = QualifiedDustOutput> {
        self.dust_utxos.values().filter_map(|v| {
            if v.pending_until.is_none() {
                Some(v.utxo)
            } else {
                None
            }
        })
    }

    pub fn find_utxo_by_nullifier(&self, nullifier: DustNullifier) -> Option<QualifiedDustOutput> {
        self.dust_utxos.get(&nullifier).and_then(|state| {
            if state.pending_until.is_none() {
                Some(state.utxo)
            } else {
                None
            }
        })
    }

    pub fn add_utxo(
        &self,
        nullifier: &DustNullifier,
        utxo: &QualifiedDustOutput,
        pending_until: Option<Timestamp>,
    ) -> Result<Self, DustSpendError> {
        let mut state = self.clone();
        state.dust_utxos = state.dust_utxos.insert(
            *nullifier,
            DustWalletUtxoState {
                utxo: *utxo,
                pending_until,
            },
        );
        Ok(state)
    }

    pub fn remove_utxo(&self, nullifier: &DustNullifier) -> Result<Self, DustSpendError> {
        let mut state = self.clone();
        state.dust_utxos = state.dust_utxos.remove(nullifier);
        Ok(state)
    }

    pub fn successor_utxo(
        &self,
        utxo: &QualifiedDustOutput,
        now: &Timestamp,
        subtract_fee: u128,
        new_commitment_index: u64,
        sk: &DustSecretKey,
    ) -> Result<QualifiedDustOutput, DustLocalStateError> {
        let gen_idx = self.night_indices.get(&utxo.backing_night).ok_or(
            DustLocalStateError::BackingNightNotFound {
                backing_night: utxo.backing_night,
            },
        )?;

        let gen_info = self.generating_tree.index(*gen_idx).unwrap().1;
        let v_pre_spend = DustOutput::from(*utxo).updated_value(gen_info, *now, &self.params);
        let v_now = v_pre_spend.saturating_sub(subtract_fee);
        let qdo_new = QualifiedDustOutput {
            backing_night: utxo.backing_night,
            ctime: *now,
            initial_value: v_now,
            seq: utxo.seq + 1,
            nonce: dust_nonce(&utxo.backing_night, utxo.seq + 1, sk),
            owner: utxo.owner,
            mt_index: new_commitment_index,
        };
        Ok(qdo_new)
    }

    pub fn generation_info(&self, qdo: &QualifiedDustOutput) -> Option<DustGenerationInfo> {
        Some(
            *self
                .generating_tree
                .index(*self.night_indices.get(&qdo.backing_night)?)?
                .1,
        )
    }

    pub fn insert_generation_info(
        &self,
        generation_index: u64,
        gen_info: DustGenerationInfo,
        initial_nonce: Option<InitialNonce>,
    ) -> Result<DustLocalState<D>, DustLocalStateError> {
        if generation_index != self.generating_tree_first_free {
            return Err(DustLocalStateError::NonLinearInsertion {
                expected_next: self.generating_tree_first_free,
                received: generation_index,
                tree_name: "dust generation",
            });
        }

        let mut state = self.clone();

        state.generating_tree = state
            .generating_tree
            .try_update_hash(generation_index, gen_info.merkle_hash(), gen_info)
            .map_err(DustLocalStateError::MerkleTreeError)?;
        state.generating_tree_first_free += 1;
        if let Some(initial_nonce) = initial_nonce {
            // TODO: shall we validate initial_nonce == gen_info.nonce ?
            state.night_indices = state.night_indices.insert(initial_nonce, generation_index);
        } else {
            state.generating_tree = state
                .generating_tree
                .collapse(generation_index, generation_index);
        }
        state.generating_tree = state.generating_tree.rehash();
        Ok(state)
    }

    pub fn remove_generation_info(
        &self,
        generation_index: u64,
        gen_info: DustGenerationInfo,
    ) -> Result<DustLocalState<D>, DustLocalStateError> {
        let actual_gen_info = self
            .generating_tree
            .index(generation_index)
            .ok_or(DustLocalStateError::GenerationIndexNotFound { generation_index })?
            .1;

        if gen_info != *actual_gen_info {
            return Err(DustLocalStateError::WrongGenerationInfo { generation_index });
        }

        let mut state = self.clone();
        state.generating_tree = state
            .generating_tree
            .collapse(generation_index, generation_index);
        state.generating_tree = state.generating_tree.rehash();
        Ok(state)
    }

    pub fn collapse_generation_tree(
        &self,
        generation_index_start: u64,
        generation_index_end: u64,
    ) -> Result<DustLocalState<D>, DustLocalStateError> {
        let mut state = self.clone();
        state.generating_tree = state
            .generating_tree
            .collapse(generation_index_start, generation_index_end);
        state.generating_tree = state.generating_tree.rehash();
        Ok(state)
    }

    pub fn apply_generation_collapsed_update(
        &self,
        update: &MerkleTreeCollapsedUpdate,
    ) -> Result<Self, merkle_tree::InvalidUpdate> {
        Ok(Self {
            generating_tree: self
                .generating_tree
                .apply_collapsed_update(update)?
                .rehash(),
            generating_tree_first_free: u64::max(self.generating_tree_first_free, update.end + 1),
            ..self.clone()
        })
    }

    pub fn insert_commitment(
        &self,
        commitment_index: u64,
        qdo: QualifiedDustOutput,
        own_qdo: bool,
    ) -> Result<DustLocalState<D>, DustLocalStateError> {
        if commitment_index != self.commitment_tree_first_free {
            return Err(DustLocalStateError::NonLinearInsertion {
                expected_next: self.commitment_tree_first_free,
                received: commitment_index,
                tree_name: "commitment",
            });
        }

        let mut state = self.clone();

        state.commitment_tree = state
            .commitment_tree
            .try_update_hash(commitment_index, qdo.commitment().into(), ())
            .map_err(DustLocalStateError::MerkleTreeError)?;
        state.commitment_tree_first_free += 1;
        if !own_qdo {
            state.commitment_tree = state
                .commitment_tree
                .collapse(commitment_index, commitment_index);
        }
        state.commitment_tree = state.commitment_tree.rehash();
        Ok(state)
    }

    pub fn remove_commitment(
        &self,
        commitment_index: u64,
    ) -> Result<DustLocalState<D>, DustLocalStateError> {
        if self.commitment_tree.index(commitment_index).is_none() {
            return Err(DustLocalStateError::CommitmentIndexNotFound { commitment_index });
        }

        let mut state = self.clone();
        state.commitment_tree = state
            .commitment_tree
            .collapse(commitment_index, commitment_index);
        state.commitment_tree = state.commitment_tree.rehash();
        Ok(state)
    }

    pub fn collapse_commitment_tree(
        &self,
        commitment_index_start: u64,
        commitment_index_end: u64,
    ) -> Result<DustLocalState<D>, DustLocalStateError> {
        let mut state = self.clone();
        state.commitment_tree = state
            .commitment_tree
            .collapse(commitment_index_start, commitment_index_end);
        state.commitment_tree = state.commitment_tree.rehash();
        Ok(state)
    }

    pub fn apply_commitment_collapsed_update(
        &self,
        update: &MerkleTreeCollapsedUpdate,
    ) -> Result<Self, merkle_tree::InvalidUpdate> {
        Ok(Self {
            commitment_tree: self
                .commitment_tree
                .apply_collapsed_update(update)?
                .rehash(),
            commitment_tree_first_free: u64::max(self.commitment_tree_first_free, update.end + 1),
            ..self.clone()
        })
    }

    pub fn spend(
        &self,
        sk: &DustSecretKey,
        utxo: &QualifiedDustOutput,
        v_fee: u128,
        ctime: Timestamp,
    ) -> Result<(Self, DustSpend<ProofPreimageMarker, D>), DustSpendError> {
        let mut state = self.clone();
        let old_nullifier = utxo.nullifier(sk);
        let gen_idx = *self
            .night_indices
            .get(&utxo.backing_night)
            .ok_or(DustSpendError::BackingNightNotFound(Box::new(*utxo)))?;
        let gen_info = self
            .generating_tree
            .index(gen_idx)
            .ok_or(DustSpendError::BackingNightNotFound(Box::new(*utxo)))?
            .1;
        // TODO: Fixme: This is assuming that `generating_tree` *is* associated
        // with `ctime`. That seems backwards? We should figure out what `ctime`
        // these trees *are* associated with, then use that.
        let gen_path = self
            .generating_tree
            .path_for_leaf(gen_idx, gen_info.merkle_hash())
            .map_err(|_| DustSpendError::BackingNightNotFound(Box::new(*utxo)))?;
        let old_com = utxo.commitment();
        let old_nul = utxo.nullifier(sk);
        let com_path = self
            .commitment_tree
            .path_for_leaf(utxo.mt_index, HashOutput::from(old_com))
            .map_err(|_| DustSpendError::DustUtxoNotTracked(Box::new(*utxo)))?;
        let v_new = DustOutput::from(*utxo).updated_value(gen_info, ctime, &self.params);
        if v_fee > v_new {
            return Err(DustSpendError::NotEnoughDust {
                available: v_new,
                required: v_fee,
            });
        }
        let new_output = DustOutput {
            ctime,
            initial_value: v_new - v_fee,
            owner: utxo.owner,
            nonce: dust_nonce(&utxo.backing_night, utxo.seq + 1, sk),
            seq: utxo.seq + 1,
        };
        let new_commitment = new_output.commitment();
        let mut utxo_entry = *state
            .dust_utxos
            .get(&old_nullifier)
            .ok_or(DustSpendError::DustUtxoNotTracked(Box::new(*utxo)))?;
        utxo_entry.pending_until = Some(ctime + self.params.dust_grace_period);
        state.dust_utxos = state.dust_utxos.insert(old_nullifier, utxo_entry);
        let inputs = (
            DustOutput::from(*utxo),
            sk.clone(),
            *gen_info,
            com_path.clone(),
            gen_path.clone(),
            utxo.backing_night,
            utxo.seq,
        )
            .field_vec();
        let mut prog = Vec::new();
        // Check commitment merkle tree root
        prog.extend::<[Op<ResultModeGather, D>; 6]>(HistoricMerkleTree_check_root!(
            [Key::Value(0u8.into())],
            false,
            32,
            Fr,
            self.commitment_tree
                .root()
                .ok_or(DustSpendError::MerkleTreeNotRehashed("dust commitment"))?
        ));
        // Check generation merkle tree root
        prog.extend(HistoricMerkleTree_check_root!(
            [Key::Value(1u8.into())],
            false,
            32,
            Fr,
            self.generating_tree
                .root()
                .ok_or(DustSpendError::MerkleTreeNotRehashed(
                    "dust generation info"
                ))?
        ));
        // Read spend
        prog.extend(Cell_read!([Key::Value(5u8.into())], false, DustSpend));
        // Read ctime
        prog.extend(Cell_read!([Key::Value(4u8.into())], false, u64));
        // Read dust parameters
        prog.extend(Cell_read!([Key::Value(3u8.into())], false, DustParameters));
        // Insert old nullifier
        prog.extend(Set_insert!([Key::Value(2u8.into())], false, Fr, old_nul.0));
        // Read ctime
        prog.extend(Cell_read!([Key::Value(4u8.into())], false, u64));
        // Insert new commitment
        prog.extend(HistoricMerkleTree_insert_hash!(
            [Key::Value(0u8.into())],
            false,
            32,
            Fr,
            HashOutput::from(new_commitment)
        ));
        let mut public_transcript_inputs = vec![];
        let erased_spend = DustSpend::<(), D> {
            v_fee,
            old_nullifier,
            new_commitment,
            proof: (),
        };
        for op in with_outputs(
            prog.into_iter(),
            [
                true.into(),                 // commitment root check
                true.into(),                 // nullifier root check
                erased_spend.clone().into(), // dust spend read
                ctime.into(),                // ctime read
                self.params.into(),          // parameter read
                ctime.into(),                // ctime read
            ]
            .into_iter(),
        ) {
            op.field_repr(&mut public_transcript_inputs);
        }
        let public_transcript_outputs =
            (true, true, erased_spend, ctime, self.params, ctime).field_vec();
        let proof = ProofPreimage {
            inputs,
            public_transcript_inputs,
            public_transcript_outputs,
            binding_input: Default::default(),
            communications_commitment: None,
            private_transcript: vec![(v_new - v_fee).into()],
            key_location: KeyLocation(std::borrow::Cow::Borrowed("midnight/dust/spend")),
        };
        Ok((
            state,
            DustSpend {
                v_fee,
                old_nullifier,
                new_commitment,
                proof,
            },
        ))
    }

    pub fn process_ttls(&self, time: Timestamp) -> Self {
        let mut state = self.clone();
        state.dust_utxos = state
            .dust_utxos
            .iter()
            .filter_map(|utxo| {
                let nul = *utxo.0;
                let mut utxo = *utxo.1;
                let gen_idx = *self.night_indices.get(&utxo.utxo.backing_night)?;
                let gen_info = self.generating_tree.index(gen_idx)?.1;
                let v_new = DustOutput::from(utxo.utxo).updated_value(gen_info, time, &self.params);
                if utxo.pending_until.map(|ptime| ptime <= time) == Some(true) {
                    utxo.pending_until = None;
                }
                if v_new == 0 && time > utxo.utxo.ctime {
                    None
                } else {
                    Some((nul, utxo))
                }
            })
            .collect();
        state
    }

    pub fn replay_events<'a>(
        &self,
        sk: &DustSecretKey,
        events: impl IntoIterator<Item = &'a Event<D>>,
    ) -> Result<Self, EventReplayError> {
        self.replay_events_with_changes(sk, events)
            .map(|w| w.result)
    }

    pub fn replay_events_with_changes<'a>(
        &self,
        sk: &DustSecretKey,
        events: impl IntoIterator<Item = &'a Event<D>>,
    ) -> Result<WithDustStateChanges<Self>, EventReplayError> {
        let pk = DustPublicKey::from(sk.clone());
        let (mut res, gen_collapses) = events.into_iter().try_fold(
            (WithDustStateChanges::new((*self).clone()), Vec::new()),
            |(mut acc, mut gen_collapses), event| {
                let maybe_change = match &event.content {
                    EventDetails::DustInitialUtxo {
                        output,
                        generation,
                        generation_index,
                        block_time,
                    } => {
                        if *generation_index != acc.result.generating_tree_first_free {
                            return Err(EventReplayError::NonLinearInsertion {
                                expected_next: acc.result.generating_tree_first_free,
                                received: *generation_index,
                                tree_name: "dust generation",
                            });
                        }
                        acc.result.generating_tree = acc
                            .result
                            .generating_tree
                            .try_update_hash(
                                *generation_index,
                                generation.merkle_hash(),
                                *generation,
                            )
                            .map_err(EventReplayError::MerkleTreeError)?;
                        acc.result.generating_tree_first_free += 1;
                        if output.mt_index != acc.result.commitment_tree_first_free {
                            return Err(EventReplayError::NonLinearInsertion {
                                expected_next: acc.result.commitment_tree_first_free,
                                received: output.mt_index,
                                tree_name: "dust commitment",
                            });
                        }
                        acc.result.commitment_tree = acc
                            .result
                            .commitment_tree
                            .try_update_hash(output.mt_index, output.commitment().into(), ())
                            .map_err(EventReplayError::MerkleTreeError)?;
                        acc.result.commitment_tree_first_free += 1;
                        let maybe_change = if pk == output.owner {
                            acc.result.night_indices = acc
                                .result
                                .night_indices
                                .insert(output.backing_night, *generation_index);
                            acc.result.dust_utxos = acc.result.dust_utxos.insert(
                                output.nullifier(sk),
                                DustWalletUtxoState {
                                    utxo: *output,
                                    pending_until: None,
                                },
                            );
                            Some(DustStateChanges {
                                received_utxos: vec![*output],
                                spent_utxos: vec![],
                                source: event.source.transaction_hash,
                            })
                        } else {
                            // Carry out generation collapses *after* applying all the events,
                            // because otherwise we might not have information around to process
                            // partial dtime updates due to these only being rehashed on block
                            // boundaries.
                            gen_collapses.push(*generation_index);
                            acc.result.commitment_tree = acc
                                .result
                                .commitment_tree
                                .collapse(output.mt_index, output.mt_index);
                            None
                        };
                        if *block_time < acc.result.sync_time {
                            return Err(EventReplayError::EventForPastTime {
                                synced: acc.result.sync_time,
                                event: *block_time,
                            });
                        }
                        acc.result.sync_time = *block_time;
                        maybe_change
                    }
                    EventDetails::ParamChange(params) => {
                        acc.result.params = params.dust;
                        None
                    }
                    EventDetails::DustSpendProcessed {
                        commitment,
                        commitment_index,
                        nullifier,
                        v_fee,
                        declared_time,
                        block_time,
                    } => {
                        if *commitment_index != acc.result.commitment_tree_first_free {
                            return Err(EventReplayError::NonLinearInsertion {
                                expected_next: acc.result.commitment_tree_first_free,
                                received: *commitment_index,
                                tree_name: "dust commitment",
                            });
                        }
                        acc.result.commitment_tree = acc
                            .result
                            .commitment_tree
                            .try_update_hash(*commitment_index, (*commitment).into(), ())
                            .map_err(EventReplayError::MerkleTreeError)?;
                        acc.result.commitment_tree_first_free += 1;
                        let maybe_change = if let Some(utxo) = acc.result.dust_utxos.get(nullifier)
                        {
                            if let Some(gen_idx) =
                                acc.result.night_indices.get(&utxo.utxo.backing_night)
                            {
                                let gen_info =
                                    acc.result.generating_tree.index(*gen_idx).unwrap().1;
                                let v_pre_spend = DustOutput::from(utxo.utxo).updated_value(
                                    gen_info,
                                    *declared_time,
                                    &acc.result.params,
                                );
                                let v_now = v_pre_spend.saturating_sub(*v_fee);
                                let spent_utxo = utxo.utxo;
                                acc.result.dust_utxos = acc.result.dust_utxos.remove(nullifier);
                                let qdo_new = QualifiedDustOutput {
                                    backing_night: spent_utxo.backing_night,
                                    ctime: *declared_time,
                                    initial_value: v_now,
                                    seq: spent_utxo.seq + 1,
                                    nonce: dust_nonce(
                                        &spent_utxo.backing_night,
                                        spent_utxo.seq + 1,
                                        sk,
                                    ),
                                    owner: spent_utxo.owner,
                                    mt_index: *commitment_index,
                                };
                                acc.result.dust_utxos = acc.result.dust_utxos.insert(
                                    qdo_new.nullifier(sk),
                                    DustWalletUtxoState {
                                        utxo: qdo_new,
                                        pending_until: None,
                                    },
                                );
                                Some(DustStateChanges {
                                    received_utxos: vec![qdo_new],
                                    spent_utxos: vec![spent_utxo],
                                    source: event.source.transaction_hash,
                                })
                            } else {
                                error!("Unable to find backing NIGHT");
                                None
                            }
                        } else {
                            acc.result.commitment_tree = acc
                                .result
                                .commitment_tree
                                .collapse(*commitment_index, *commitment_index);
                            None
                        };
                        if *block_time < acc.result.sync_time {
                            return Err(EventReplayError::EventForPastTime {
                                synced: acc.result.sync_time,
                                event: *block_time,
                            });
                        }
                        acc.result.sync_time = *block_time;
                        maybe_change
                    }
                    EventDetails::DustGenerationDtimeUpdate { update, block_time } => {
                        debug_assert!(update.path.iter().all(|entry| entry.hash.is_some()));
                        acc.result.generating_tree = acc
                            .result
                            .generating_tree
                            .update_from_evidence(update.clone())?;
                        if *block_time < acc.result.sync_time {
                            return Err(EventReplayError::EventForPastTime {
                                synced: acc.result.sync_time,
                                event: *block_time,
                            });
                        }
                        acc.result.sync_time = *block_time;
                        None
                    }
                    _ => None,
                };
                Ok((acc.maybe_add_change(maybe_change), gen_collapses))
            },
        )?;
        for collapse in gen_collapses {
            res.result.generating_tree = res.result.generating_tree.collapse(collapse, collapse);
        }
        res.result.commitment_tree = res.result.commitment_tree.rehash();
        res.result.generating_tree = res.result.generating_tree.rehash();
        Ok(res)
    }
}

macro_rules! exptfile {
    ($name:literal, $desc:literal) => {
        (
            concat!("dust/", midnight_ledger_static::version!(), "/", $name),
            base_crypto::data_provider::hexhash(
                &include_bytes!(concat!("../static/dust/", $name, ".sha256"))
                    .split_at(64)
                    .0,
            ),
            $desc,
        )
    };
}

/// Files provided by Midnight's data provider for Dust.
pub const DUST_EXPECTED_FILES: &[(&str, [u8; 32], &str)] = &[
    exptfile!("spend.prover", "zero-knowledge proving key for Dust spends"),
    exptfile!(
        "spend.verifier",
        "zero-knowledge verifying key for Dust spends"
    ),
    exptfile!("spend.bzkir", "ZKIR source for Dust spends"),
];

pub const DUST_SPEND_PROOF_SIZE: usize = 2_912;
pub const DUST_SPEND_PIS: usize = 138;

#[cfg(test)]
mod tests {
    use super::{DustSecretKey, Seed};
    use hex::FromHex;
    use serde::de::{Error, Unexpected};
    use serde::{Deserialize, Deserializer};
    use std::fs;

    #[cfg(feature = "proving")]
    #[tokio::test]
    async fn test_proof_size() {
        use base_crypto::rng::SplittableRng;
        use rand::{Rng, SeedableRng, rngs::StdRng};
        use storage::db::InMemoryDB;
        use transient_crypto::commitment::{Pedersen, PedersenRandomness};
        use zkir_v2::LocalProvingProvider;

        use crate::{
            dust::DUST_SPEND_PROOF_SIZE,
            test_utilities::{TestState, test_resolver},
        };

        let mut rng = StdRng::seed_from_u64(0x42);
        let mut state = TestState::<InMemoryDB>::new(&mut rng);
        state.give_fee_token(&mut rng, 1).await;
        let utxo = state.dust.utxos().next().unwrap();
        let dust_spend = state
            .dust
            .spend(&state.dust_key, &utxo, 42, state.time)
            .unwrap()
            .1;
        let resolver = test_resolver("");
        let prover = LocalProvingProvider {
            rng: rng.split(),
            params: &resolver,
            resolver: &resolver,
        };
        let binding = Pedersen::from(rng.r#gen::<PedersenRandomness>());
        let proven_dust_spend = dust_spend.prove(prover, 0, binding).await.unwrap();
        assert_eq!(proven_dust_spend.proof.0.len(), DUST_SPEND_PROOF_SIZE);
    }

    struct WrappedSeed(pub Seed);
    impl<'de> Deserialize<'de> for WrappedSeed {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s = <String as Deserialize>::deserialize(deserializer)?;
            let as_arr: [u8; 32] = <[u8; 32]>::from_hex(s.clone())
                .map_err(|_err| Error::invalid_value(Unexpected::Str(&s), &"hex string"))?;
            Ok(WrappedSeed(as_arr))
        }
    }

    struct HexArr64([u8; 64]);
    impl<'de> Deserialize<'de> for HexArr64 {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s = <String as Deserialize>::deserialize(deserializer)?;
            let arr: [u8; 64] = <[u8; 64]>::from_hex(s.clone()).map_err(|_err| {
                println!("{}", _err);
                Error::invalid_value(Unexpected::Str(&s), &"hex string")
            })?;
            Ok(HexArr64(arr))
        }
    }

    struct HexArr32([u8; 32]);
    impl<'de> Deserialize<'de> for HexArr32 {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s = <String as Deserialize>::deserialize(deserializer)?;
            let arr: [u8; 32] = <[u8; 32]>::from_hex(s.clone()).map_err(|_err| {
                println!("{}", _err);
                Error::invalid_value(Unexpected::Str(&s), &"hex string")
            })?;
            Ok(HexArr32(arr))
        }
    }

    #[allow(non_snake_case, dead_code)]
    #[derive(Deserialize)]
    struct EncryptionVectorEntry {
        secretKeyRepr: HexArr32,
        secretKeyDecimal: String,
        secretKeyIntermediateBytes: HexArr64,
    }

    #[allow(non_snake_case)]
    #[derive(Deserialize)]
    struct VectorEntry {
        seed: WrappedSeed,
        dust: EncryptionVectorEntry,
    }

    struct TestVectors(Vec<VectorEntry>);
    impl TestVectors {
        fn load() -> TestVectors {
            let raw = fs::read("key-derivation-test-vectors.json").unwrap();
            let parsed: Vec<VectorEntry> = serde_json::from_slice(raw.as_slice()).unwrap();
            TestVectors(parsed)
        }
    }

    #[test]
    fn encryption_key_derivation_matches_test_vectors() {
        use transient_crypto::curve::Fr;
        let test_vectors = TestVectors::load();

        for entry in test_vectors.0 {
            let dsk_computed = DustSecretKey::derive_secret_key(&entry.seed.0);
            let dsk_reference =
                DustSecretKey(Fr::from_le_bytes(&entry.dust.secretKeyRepr.0).unwrap());
            let intermediate_computed =
                DustSecretKey::sample_bytes(&entry.seed.0, 64, b"midnight:dsk");
            let dsk_from_intermediate = DustSecretKey(Fr::from_uniform_bytes(
                &entry.dust.secretKeyIntermediateBytes.0,
            ));

            println!("Encryption Keys:");
            println!("  seed:                  {:?}", entry.seed.0);
            println!(
                "  intermediate bytes:    {:?}",
                &entry.dust.secretKeyIntermediateBytes.0
            );
            println!(
                "  intermediate computed: {:?}",
                intermediate_computed.as_slice()
            );
            println!("  computed:              {:?}", dsk_computed.repr());
            println!("  reference:             {:?}", dsk_reference.repr());
            println!("  reference raw:         {:?}", entry.dust.secretKeyRepr.0);
            println!(
                "  from intermediate:     {:?}",
                dsk_from_intermediate.repr()
            );

            assert_eq!(
                intermediate_computed.as_slice(),
                entry.dust.secretKeyIntermediateBytes.0.as_slice(),
                "Intermediate bytes do not match"
            );
            assert_eq!(
                dsk_computed.repr(),
                dsk_from_intermediate.repr(),
                "Key computed from seed does not match key computed from intermediate bytes"
            );
            assert_eq!(
                dsk_computed.repr(),
                dsk_reference.repr(),
                "Key computed from seed does not match reference"
            );
            assert_eq!(
                dsk_reference.repr(),
                dsk_from_intermediate.repr(),
                "Key computed from intermediate bytes does not match reference"
            );
        }
    }
}
