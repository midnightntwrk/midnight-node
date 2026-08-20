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

use crate::annotation::NightAnn;
use crate::dust::DUST_GENERATION_INFO_SIZE;
use crate::dust::DUST_SPEND_PIS;
use crate::dust::DUST_SPEND_PROOF_SIZE;
use crate::dust::{DustActions, DustParameters, DustState, INITIAL_DUST_PARAMETERS};
use crate::error::FeeCalculationError;
use crate::error::InvariantViolation;
use crate::error::MalformedTransaction;
use crate::verify::ProofVerificationMode;
use base_crypto::BinaryHashRepr;
use base_crypto::cost_model::RunningCost;
use base_crypto::cost_model::price_adjustment_function;
use base_crypto::cost_model::{CostDuration, FeePrices, FixedPoint, SyntheticCost};
use base_crypto::hash::HashOutput;
use base_crypto::hash::PERSISTENT_HASH_BYTES;
use base_crypto::hash::persistent_hash;
use base_crypto::repr::MemWrite;
use base_crypto::signatures::Signature;
use base_crypto::signatures::{SigningKey, VerifyingKey};
use base_crypto::time::{Duration, Timestamp};
use coin_structure::coin::NIGHT;
use coin_structure::coin::PublicAddress;
use coin_structure::coin::UserAddress;
use coin_structure::coin::{Commitment, Nonce, ShieldedTokenType, TokenType, UnshieldedTokenType};
use coin_structure::contract::ContractAddress;
use derive_where::derive_where;
use fake::Dummy;
use introspection_derive::Introspection;
use onchain_runtime::context::{BlockContext, CallContext, ClaimedContractCallsValue};
use onchain_runtime::state::ChargedState;
use onchain_runtime::state::ContractOperation;
use onchain_runtime::state::{ContractMaintenanceAuthority, ContractState, EntryPointBuf};
use onchain_runtime::transcript::Transcript;
use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use serialize::{
    self, Deserializable, Serializable, Tagged, tag_enforcement_test, tagged_serialize,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::{self, Formatter};
use std::hash::Hash;
use std::io::{Read, Write};
use std::iter::once;
use std::marker::PhantomData;
use std::ops::Deref;
use storage::Storable;
use storage::arena::{ArenaHash, ArenaKey, Sp};
use storage::db::DB;
use storage::db::InMemoryDB;
use storage::merkle_patricia_trie::Annotation;
use storage::storable::Loader;
use storage::storage::Map;
use storage::storage::{HashMap, HashSet, TimeFilterMap};
use transient_crypto::commitment::{Pedersen, PedersenRandomness, PureGeneratorPedersen};
use transient_crypto::curve::FR_BYTES;
use transient_crypto::curve::Fr;
use transient_crypto::proofs::KeyLocation;
use transient_crypto::proofs::VerifierKey;
use transient_crypto::proofs::{Proof, ProofPreimage};
use transient_crypto::repr::{FieldRepr, FromFieldRepr};
use zswap::ZSWAP_TREE_HEIGHT;
use zswap::error::MalformedOffer;
use zswap::{Input, Offer as ZswapOffer, Output, Transient};

/// A trait for things that can fit into `Signature` shaped holes
pub trait SignatureKind<D: DB>: Ord + Storable<D> + Debug + Tagged + 'static {
    /// The type of the `Signature` shaped thing
    type Signature<T>: Ord + Serializable + Deserializable + Storable<D> + Debug + Tagged;

    /// Verify a signature against a message
    fn signature_verify<T>(msg: &[u8], key: VerifyingKey, signature: &Self::Signature<T>) -> bool;

    fn sign<R: Rng + CryptoRng, T>(
        sk: &SigningKey,
        rng: &mut R,
        msg: &[u8],
    ) -> <Self as SignatureKind<D>>::Signature<T>;
}

impl<D: DB> SignatureKind<D> for () {
    type Signature<T> = ();

    fn signature_verify<T>(_msg: &[u8], _key: VerifyingKey, _signature: &()) -> bool {
        true
    }

    fn sign<R: Rng + CryptoRng, T>(_: &SigningKey, _: &mut R, _: &[u8]) {}
}

impl<D: DB> SignatureKind<D> for Signature {
    type Signature<T> = Signature;

    fn signature_verify<T>(msg: &[u8], key: VerifyingKey, signature: &Signature) -> bool {
        key.verify(msg, signature)
    }

    fn sign<R: Rng + CryptoRng, T>(sk: &SigningKey, rng: &mut R, msg: &[u8]) -> Signature {
        sk.sign(rng, msg)
    }
}

pub trait BindingKind<S: SignatureKind<D>, P: ProofKind<D>, D: DB>: Sync + Send {
    fn when_sealed(
        f: Result<impl Fn() -> Result<(), MalformedTransaction<D>>, MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>>;
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> BindingKind<S, P, D> for Pedersen {
    fn when_sealed(
        f: Result<impl Fn() -> Result<(), MalformedTransaction<D>>, MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        f?;
        Ok(())
    }
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> BindingKind<S, P, D> for PedersenRandomness {
    fn when_sealed(
        f: Result<impl Fn() -> Result<(), MalformedTransaction<D>>, MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        f?;
        Ok(())
    }
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> BindingKind<S, P, D> for PureGeneratorPedersen {
    fn when_sealed(
        f: Result<impl Fn() -> Result<(), MalformedTransaction<D>>, MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        f?()
    }
}

pub trait PedersenDowngradeable<D: DB>:
    Into<Pedersen> + Clone + PartialEq + Eq + PartialOrd + Ord + Serialize
{
    fn downgrade(&self) -> Pedersen;

    #[allow(clippy::result_large_err)]
    fn valid(&self, _challenge_pre: &[u8]) -> Result<(), MalformedTransaction<D>>;
}

pub trait PedersenUpgradeable<D: DB>:
    Into<Pedersen> + Clone + PartialEq + Eq + PartialOrd + Ord + Serialize + PedersenDowngradeable<D>
{
    fn upgrade<R: Rng + CryptoRng>(
        &self,
        rng: &mut R,
        challenge_pre: &[u8],
    ) -> PureGeneratorPedersen;
}

impl<D: DB> PedersenDowngradeable<D> for PureGeneratorPedersen {
    fn downgrade(&self) -> Pedersen {
        Pedersen::from(self.clone())
    }

    fn valid(&self, challenge_pre: &[u8]) -> Result<(), MalformedTransaction<D>> {
        if PureGeneratorPedersen::valid(self, challenge_pre) {
            Ok(())
        } else {
            Err(MalformedTransaction::<D>::InvalidSchnorrProof)
        }
    }
}

impl<D: DB> PedersenUpgradeable<D> for PureGeneratorPedersen {
    fn upgrade<R: Rng + CryptoRng>(
        &self,
        _rng: &mut R,
        _challenge_pre: &[u8],
    ) -> PureGeneratorPedersen {
        self.clone()
    }
}

impl<D: DB> PedersenDowngradeable<D> for PedersenRandomness {
    fn downgrade(&self) -> Pedersen {
        Pedersen::from(*self)
    }

    fn valid(&self, _challenge_pre: &[u8]) -> Result<(), MalformedTransaction<D>> {
        Ok(())
    }
}

impl<D: DB> PedersenUpgradeable<D> for PedersenRandomness {
    fn upgrade<R: Rng + CryptoRng>(
        &self,
        rng: &mut R,
        challenge_pre: &[u8],
    ) -> PureGeneratorPedersen {
        PureGeneratorPedersen::new_from(rng, self, challenge_pre)
    }
}

impl<D: DB> PedersenDowngradeable<D> for Pedersen {
    fn downgrade(&self) -> Pedersen {
        *self
    }

    fn valid(&self, _challenge_pre: &[u8]) -> Result<(), MalformedTransaction<D>> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Storable)]
#[storable(base)]
#[tag = "proof-preimage-versioned"]
#[non_exhaustive]
pub enum ProofPreimageVersioned {
    V2(std::sync::Arc<ProofPreimage>),
}

impl Serializable for ProofPreimageVersioned {
    fn serialize(&self, writer: &mut impl Write) -> std::io::Result<()> {
        match self {
            ProofPreimageVersioned::V2(preimage) => {
                Serializable::serialize(&1u8, writer)?;
                preimage.serialize(writer)
            }
        }
    }
    fn serialized_size(&self) -> usize {
        match self {
            ProofPreimageVersioned::V2(preimage) => preimage.serialized_size() + 1,
        }
    }
}

impl Deserializable for ProofPreimageVersioned {
    fn deserialize(reader: &mut impl Read, recursion_depth: u32) -> std::io::Result<Self> {
        let discrim = Deserializable::deserialize(reader, recursion_depth)?;
        match discrim {
            0u8 => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid old discriminant for ProofPreimageVersioned: {discrim}"),
            )),
            1 => Ok(ProofPreimageVersioned::V2(std::sync::Arc::new(
                ProofPreimage::deserialize(reader, recursion_depth)?,
            ))),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown discriminant for ProofPreimageVersioned: {discrim}"),
            )),
        }
    }
}

impl Tagged for ProofPreimageVersioned {
    fn tag() -> std::borrow::Cow<'static, str> {
        "proof-preimage-versioned".into()
    }
    fn tag_unique_factor() -> String {
        format!("[[],{}]", ProofPreimage::tag())
    }
}

tag_enforcement_test!(ProofPreimageVersioned);

impl ProofPreimageVersioned {
    pub fn key_location(&self) -> &KeyLocation {
        match self {
            ProofPreimageVersioned::V2(ppi) => &ppi.key_location,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Storable, Introspection)]
#[storable(base)]
#[tag = "proof-versioned"]
#[non_exhaustive]
pub enum ProofVersioned {
    V2(Proof),
}

impl Serializable for ProofVersioned {
    fn serialize(&self, writer: &mut impl Write) -> std::io::Result<()> {
        match self {
            ProofVersioned::V2(proof) => {
                Serializable::serialize(&1u8, writer)?;
                proof.serialize(writer)
            }
        }
    }
    fn serialized_size(&self) -> usize {
        match self {
            ProofVersioned::V2(proof) => proof.serialized_size() + 1,
        }
    }
}

impl Deserializable for ProofVersioned {
    fn deserialize(reader: &mut impl Read, recursion_depth: u32) -> std::io::Result<Self> {
        let discrim = Deserializable::deserialize(reader, recursion_depth)?;
        match discrim {
            0u8 => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid old discriminant for ProofVersioned: {discrim}"),
            )),
            1 => Ok(ProofVersioned::V2(Proof::deserialize(
                reader,
                recursion_depth,
            )?)),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown discriminant for ProofVersioned: {discrim}"),
            )),
        }
    }
}

impl Tagged for ProofVersioned {
    fn tag() -> std::borrow::Cow<'static, str> {
        "proof-versioned".into()
    }
    fn tag_unique_factor() -> String {
        format!("[[],{}]", Proof::tag())
    }
}

tag_enforcement_test!(ProofVersioned);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serializable, Storable)]
#[tag = "proof"]
#[storable(base)]
pub struct ProofMarker;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serializable, Storable)]
#[tag = "proof-preimage"]
#[storable(base)]
pub struct ProofPreimageMarker;

pub trait ProofKind<D: DB>: Ord + Storable<D> + Serializable + Deserializable + Tagged {
    type Pedersen: PedersenDowngradeable<D> + Serializable + Deserializable + Tagged;
    type Proof: Clone
        + PartialEq
        + Eq
        + PartialOrd
        + Ord
        + Serializable
        + Deserializable
        + Tagged
        + Storable<D>;
    type LatestProof: Clone
        + PartialEq
        + Eq
        + Ord
        + Serializable
        + Deserializable
        + Hash
        + Storable<D>
        + Tagged
        + Into<Self::Proof>;
    fn zswap_well_formed(
        offer: &zswap::Offer<Self::LatestProof, D>,
        segment: u16,
    ) -> Result<Pedersen, MalformedOffer>;
    fn zswap_claim_well_formed(
        claim: &zswap::AuthorizedClaim<Self::LatestProof>,
    ) -> Result<(), MalformedOffer>;
    #[allow(clippy::result_large_err)]
    fn proof_verify(
        op: &ContractOperation,
        proof: &Self::Proof,
        pis: Vec<Fr>,
        call: &ContractCall<Self, D>,
        mode: ProofVerificationMode,
    ) -> Result<(), MalformedTransaction<D>>;
    #[allow(clippy::result_large_err)]
    fn latest_proof_verify(
        op: &VerifierKey,
        proof: &Self::LatestProof,
        pis: Vec<Fr>,
        mode: ProofVerificationMode,
    ) -> Result<(), MalformedTransaction<D>>;
    /// Provides the transaction size, real for proven transactions, and crudely
    /// estimated for unproven.
    fn estimated_tx_size<
        S: SignatureKind<D>,
        B: Storable<D> + PedersenDowngradeable<D> + Serializable,
    >(
        tx: &Transaction<S, Self, B, D>,
    ) -> usize;
}

impl From<Proof> for ProofVersioned {
    fn from(proof: Proof) -> Self {
        Self::V2(proof)
    }
}

impl<D: DB> ProofKind<D> for ProofMarker {
    type Pedersen = PureGeneratorPedersen;
    type Proof = ProofVersioned;
    type LatestProof = Proof;
    fn zswap_well_formed(
        offer: &zswap::Offer<Self::LatestProof, D>,
        segment: u16,
    ) -> Result<Pedersen, MalformedOffer> {
        offer.well_formed(segment)
    }
    fn zswap_claim_well_formed(
        claim: &zswap::AuthorizedClaim<Self::LatestProof>,
    ) -> Result<(), MalformedOffer> {
        claim.well_formed()
    }
    #[cfg(not(feature = "proof-verifying"))]
    fn proof_verify(
        _op: &ContractOperation,
        _proof: &Self::Proof,
        _pis: Vec<Fr>,
        _call: &ContractCall<Self, D>,
        _mode: ProofVerificationMode,
    ) -> Result<(), MalformedTransaction<D>> {
        Ok(())
    }
    #[cfg(feature = "proof-verifying")]
    fn proof_verify(
        op: &ContractOperation,
        proof: &Self::Proof,
        pis: Vec<Fr>,
        call: &ContractCall<Self, D>,
        mode: ProofVerificationMode,
    ) -> Result<(), MalformedTransaction<D>> {
        use transient_crypto::proofs::PARAMS_VERIFIER;

        let vk = match &op.v2 {
            Some(vk) => vk,
            None => {
                warn!("missing verifier key");
                return Err(MalformedTransaction::<D>::VerifierKeyNotPresent {
                    address: call.address,
                    operation: call.entry_point.clone(),
                });
            }
        };

        if op.v2.is_some() && !matches!(proof, ProofVersioned::V2(_)) {
            return Err(MalformedTransaction::<D>::UnsupportedProofVersion {
                op_version: "V2".to_string(),
            });
        }

        match proof {
            ProofVersioned::V2(proof) => match mode {
                #[cfg(feature = "mock-verify")]
                ProofVerificationMode::CalibratedMock => vk
                    .mock_verify(pis.into_iter())
                    .map_err(MalformedTransaction::<D>::InvalidProof),
                _ => vk
                    .verify(&PARAMS_VERIFIER, proof, pis.into_iter())
                    .map_err(MalformedTransaction::<D>::InvalidProof),
            },
        }
    }
    #[allow(clippy::result_large_err)]
    fn latest_proof_verify(
        vk: &VerifierKey,
        proof: &Self::LatestProof,
        pis: Vec<Fr>,
        mode: ProofVerificationMode,
    ) -> Result<(), MalformedTransaction<D>> {
        use transient_crypto::proofs::PARAMS_VERIFIER;

        match mode {
            #[cfg(feature = "mock-verify")]
            ProofVerificationMode::CalibratedMock => vk
                .mock_verify(pis.into_iter())
                .map_err(MalformedTransaction::<D>::InvalidProof),
            _ => vk
                .verify(&PARAMS_VERIFIER, proof, pis.into_iter())
                .map_err(MalformedTransaction::<D>::InvalidProof),
        }
    }
    fn estimated_tx_size<
        S: SignatureKind<D>,
        B: Storable<D> + PedersenDowngradeable<D> + Serializable,
    >(
        tx: &Transaction<S, Self, B, D>,
    ) -> usize {
        tx.serialized_size()
    }
}

impl From<ProofPreimage> for ProofPreimageVersioned {
    fn from(proof: ProofPreimage) -> Self {
        Self::V2(std::sync::Arc::new(proof))
    }
}

impl<D: DB> ProofKind<D> for ProofPreimageMarker {
    type Pedersen = PedersenRandomness;
    type Proof = ProofPreimageVersioned;
    type LatestProof = ProofPreimage;
    fn zswap_well_formed(
        offer: &zswap::Offer<Self::LatestProof, D>,
        segment: u16,
    ) -> Result<Pedersen, MalformedOffer> {
        offer.well_formed(segment)
    }
    fn zswap_claim_well_formed(
        _: &zswap::AuthorizedClaim<Self::LatestProof>,
    ) -> Result<(), MalformedOffer> {
        Ok(())
    }
    fn proof_verify(
        _: &ContractOperation,
        _: &Self::Proof,
        _: Vec<Fr>,
        _: &ContractCall<Self, D>,
        _: ProofVerificationMode,
    ) -> Result<(), MalformedTransaction<D>> {
        Ok(())
    }
    #[allow(clippy::result_large_err)]
    fn latest_proof_verify(
        _: &VerifierKey,
        _: &Self::LatestProof,
        _: Vec<Fr>,
        _: ProofVerificationMode,
    ) -> Result<(), MalformedTransaction<D>> {
        Ok(())
    }
    fn estimated_tx_size<
        S: SignatureKind<D>,
        B: Storable<D> + PedersenDowngradeable<D> + Serializable,
    >(
        tx: &Transaction<S, Self, B, D>,
    ) -> usize {
        <()>::estimated_tx_size(&tx.erase_proofs())
    }
}

impl<D: DB> ProofKind<D> for () {
    type Pedersen = Pedersen;
    type Proof = ();
    type LatestProof = ();
    fn zswap_well_formed(
        offer: &zswap::Offer<Self::LatestProof, D>,
        segment: u16,
    ) -> Result<Pedersen, MalformedOffer> {
        offer.well_formed(segment)
    }
    fn zswap_claim_well_formed(
        _: &zswap::AuthorizedClaim<Self::LatestProof>,
    ) -> Result<(), MalformedOffer> {
        Ok(())
    }
    fn proof_verify(
        _: &ContractOperation,
        _: &Self::Proof,
        _: Vec<Fr>,
        _: &ContractCall<Self, D>,
        _: ProofVerificationMode,
    ) -> Result<(), MalformedTransaction<D>> {
        Ok(())
    }
    #[allow(clippy::result_large_err)]
    fn latest_proof_verify(
        _: &VerifierKey,
        _: &Self::LatestProof,
        _: Vec<Fr>,
        _: ProofVerificationMode,
    ) -> Result<(), MalformedTransaction<D>> {
        Ok(())
    }
    fn estimated_tx_size<
        S: SignatureKind<D>,
        B: Storable<D> + PedersenDowngradeable<D> + Serializable,
    >(
        tx: &Transaction<S, Self, B, D>,
    ) -> usize {
        let size = tx.serialized_size();
        let calls = tx.calls().count();
        let dust_spends = tx
            .intents()
            .map(|(_, intent)| {
                intent
                    .dust_actions
                    .as_ref()
                    .map(|da| da.spends.len())
                    .unwrap_or(0)
            })
            .sum::<usize>();
        let (zswap_inputs, zswap_outputs) = if let Transaction::Standard(stx) = tx {
            let transients = stx.transients().count();
            (
                stx.inputs().count() + transients,
                stx.outputs().count() + transients,
            )
        } else {
            (0, 0)
        };
        size + calls * PROOF_SIZE
            + zswap_inputs * zswap::INPUT_PROOF_SIZE
            + zswap_outputs * zswap::OUTPUT_PROOF_SIZE
            + dust_spends * DUST_SPEND_PROOF_SIZE
    }
}

#[derive(Clone, Debug, PartialEq, Serializable, Storable)]
#[storable(base)]
#[tag = "output-instruction-shielded[v2]"]
pub struct OutputInstructionShielded {
    pub amount: u128,
    pub target_key: coin_structure::coin::PublicKey,
}
tag_enforcement_test!(OutputInstructionShielded);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serializable, Storable)]
#[storable(base)]
#[tag = "claim-kind[v1]"]
pub enum ClaimKind {
    Reward,
    CardanoBridge,
}
tag_enforcement_test!(ClaimKind);

impl Display for ClaimKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ClaimKind::Reward => write!(f, "rewards claim"),
            ClaimKind::CardanoBridge => write!(f, "cardano bridge claim"),
        }
    }
}

impl rand::distributions::Distribution<ClaimKind> for rand::distributions::Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> ClaimKind {
        let b: bool = rng.r#gen();
        if b {
            ClaimKind::Reward
        } else {
            ClaimKind::CardanoBridge
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serializable, Storable)]
#[storable(base)]
#[tag = "output-instruction-unshielded[v1]"]
pub struct OutputInstructionUnshielded {
    pub amount: u128,
    pub target_address: UserAddress,
    pub nonce: Nonce,
}
tag_enforcement_test!(OutputInstructionUnshielded);

impl OutputInstructionUnshielded {
    pub fn to_hash_data(self, tt: UnshieldedTokenType) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend(b"midnight:hash-output-instruction-unshielded:");
        Serializable::serialize(&tt, &mut data).expect("In-memory serialization should succeed");
        Serializable::serialize(&self.amount, &mut data)
            .expect("In-memory serialization should succeed");
        Serializable::serialize(&self.target_address, &mut data)
            .expect("In-memory serialization should succeed");
        Serializable::serialize(&self.nonce, &mut data)
            .expect("In-memory serialization should succeed");
        data
    }

    pub fn mk_intent_hash(self, tt: UnshieldedTokenType) -> IntentHash {
        IntentHash(persistent_hash(&self.to_hash_data(tt)))
    }
}

#[derive(Clone, Debug, PartialEq, Serializable)]
#[tag = "cnight-generates-dust-action-type"]
pub enum CNightGeneratesDustActionType {
    Create,
    Destroy,
}
tag_enforcement_test!(CNightGeneratesDustActionType);

#[derive(Clone, Debug, PartialEq, Serializable)]
#[tag = "cnight-generates-dust-event[v1]"]
pub struct CNightGeneratesDustEvent {
    pub value: u128,
    pub owner: crate::dust::DustPublicKey,
    pub time: Timestamp,
    pub action: CNightGeneratesDustActionType,
    pub nonce: crate::dust::InitialNonce,
}
tag_enforcement_test!(CNightGeneratesDustEvent);

#[derive(Clone, Debug, PartialEq, Serializable, Storable)]
#[storable(base)]
#[tag = "cardano-bridge[v1]"]
pub struct CardanoBridge {
    pub amount: u128,
    pub target_address: UserAddress,
    pub nonce: Nonce,
}
tag_enforcement_test!(CardanoBridge);

#[derive(Clone, Debug, PartialEq, Serializable, Storable)]
#[tag = "system-transaction[v6]"]
#[storable(base)]
#[non_exhaustive]
// TODO: Getting `Box` to serialize is a pain right now. Revisit later.
#[allow(clippy::large_enum_variant)]
pub enum SystemTransaction {
    OverwriteParameters(LedgerParameters),
    DistributeNight(ClaimKind, Vec<OutputInstructionUnshielded>),
    PayBlockRewardsToTreasury {
        amount: u128,
    },
    PayFromTreasuryShielded {
        outputs: Vec<OutputInstructionShielded>,
        nonce: HashOutput,
        token_type: ShieldedTokenType,
    },
    PayFromTreasuryUnshielded {
        outputs: Vec<OutputInstructionUnshielded>,
        token_type: UnshieldedTokenType,
    },
    DistributeReserve(u128),
    CNightGeneratesDustUpdate {
        events: Vec<CNightGeneratesDustEvent>,
    },
}
tag_enforcement_test!(SystemTransaction);

#[derive(Storable)]
#[derive_where(Clone, PartialEq, Eq)]
#[storable(db = D)]
pub struct SegIntent<D: DB>(u16, ErasedIntent<D>);

impl<D: DB> SegIntent<D> {
    pub fn into_inner(&self) -> (u16, ErasedIntent<D>) {
        (self.0, self.1.clone())
    }
}

#[derive(Storable)]
#[tag = "unshielded-offer[v1]"]
#[derive_where(Clone, PartialEq, Eq, PartialOrd, Ord; S)]
#[storable(db = D)]
pub struct UnshieldedOffer<S: SignatureKind<D>, D: DB> {
    pub inputs: storage::storage::Array<UtxoSpend, D>,
    // This will soon become the following with the introduction of Dust
    // tokenomics:
    // outputs: Vec<Either<UtxoOutput, GeneratingUtxoOutput>>,
    pub outputs: storage::storage::Array<UtxoOutput, D>,
    // Note that for S = (), this has a fixed point of ().
    // This signs the intent, and the segment ID
    pub signatures: storage::storage::Array<S::Signature<SegIntent<D>>, D>,
}
tag_enforcement_test!(UnshieldedOffer<(), InMemoryDB>);

impl<S: SignatureKind<D>, D: DB> fmt::Debug for UnshieldedOffer<S, D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnshieldedOffer")
            .field("inputs", &self.inputs)
            .field("outputs", &self.outputs)
            .field(
                "signatures",
                &format_args!("[{} signatures]", self.signatures.len()),
            )
            .finish()
    }
}

impl<S: SignatureKind<D>, D: DB> UnshieldedOffer<S, D> {
    pub fn add_signatures(
        &mut self,
        signatures: Vec<<S as SignatureKind<D>>::Signature<SegIntent<D>>>,
    ) {
        for signature in signatures {
            self.signatures = self.signatures.push(signature);
        }
    }

    pub fn erase_signatures(&self) -> UnshieldedOffer<(), D>
    where
        UnshieldedOffer<S, D>: Clone,
    {
        UnshieldedOffer {
            inputs: self.inputs.clone(),
            outputs: self.outputs.clone(),
            signatures: vec![].into(),
        }
    }
}

#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    FieldRepr,
    FromFieldRepr,
    BinaryHashRepr,
    Serializable,
    Storable,
    Dummy,
)]
#[storable(base)]
#[tag = "intent-hash"]
pub struct IntentHash(pub HashOutput);
tag_enforcement_test!(IntentHash);

impl<D: DB> ErasedIntent<D> {
    pub fn intent_hash(&self, segment_id: u16) -> IntentHash {
        IntentHash(persistent_hash(&self.data_to_sign(segment_id)))
    }
}

impl rand::distributions::Distribution<IntentHash> for rand::distributions::Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> IntentHash {
        IntentHash(rng.r#gen())
    }
}

pub type ErasedIntent<D> = Intent<(), (), Pedersen, D>;

#[derive(Storable)]
#[tag = "intent[v6]"]
#[derive_where(Clone, PartialEq, Eq; S, B, P)]
#[storable(db = D)]
pub struct Intent<S: SignatureKind<D>, P: ProofKind<D>, B: Storable<D>, D: DB> {
    pub guaranteed_unshielded_offer: Option<Sp<UnshieldedOffer<S, D>, D>>,
    pub fallible_unshielded_offer: Option<Sp<UnshieldedOffer<S, D>, D>>,
    pub actions: storage::storage::Array<ContractAction<P, D>, D>,
    pub dust_actions: Option<Sp<DustActions<S, P, D>, D>>,
    pub ttl: Timestamp,
    pub binding_commitment: B,
}
tag_enforcement_test!(Intent<(), (), Pedersen, InMemoryDB>);

impl<S: SignatureKind<D>, P: ProofKind<D>, B: Storable<D>, D: DB> Intent<S, P, B, D> {
    pub fn challenge_pre_for(&self, segment_id: u16) -> Vec<u8> {
        let mut data = ContractAction::challenge_pre_for(Vec::from(&self.actions).as_slice());
        let _ = Serializable::serialize(&segment_id, &mut data);

        data
    }
}

impl<S: SignatureKind<D> + Debug, P: ProofKind<D>, B: Storable<D>, D: DB> fmt::Debug
    for Intent<S, P, B, D>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug_struct = f.debug_struct("Intent");
        debug_struct
            .field(
                "guaranteed_unshielded_offer",
                &self.guaranteed_unshielded_offer,
            )
            .field("fallible_unshielded_offer", &self.fallible_unshielded_offer)
            .field("actions", &self.actions)
            .field("ttl", &self.ttl);
        debug_struct.field("dust_actions", &self.dust_actions);
        debug_struct
            .field("binding_commitment", &Symbol("<binding commitment>"))
            .finish()
    }
}

fn to_hash_data<D: DB>(intent: Intent<(), (), Pedersen, D>, mut data: Vec<u8>) -> Vec<u8> {
    Serializable::serialize(&intent.guaranteed_unshielded_offer, &mut data)
        .expect("In-memory serialization should succeed");
    Serializable::serialize(&intent.fallible_unshielded_offer, &mut data)
        .expect("In-memory serialization should succeed");
    Serializable::serialize(&intent.actions, &mut data)
        .expect("In-memory serialization should succeed");
    Serializable::serialize(&intent.ttl, &mut data)
        .expect("In-memory serialization should succeed");
    Serializable::serialize(&intent.binding_commitment, &mut data)
        .expect("In-memory serialization should succeed");
    data
}

impl<
    S: SignatureKind<D>,
    P: ProofKind<D>,
    B: Storable<D> + PedersenDowngradeable<D> + Serializable,
    D: DB,
> Intent<S, P, B, D>
{
    pub fn erase_proofs(&self) -> Intent<S, (), Pedersen, D>
    where
        UnshieldedOffer<S, D>: Clone,
    {
        Intent {
            guaranteed_unshielded_offer: self.guaranteed_unshielded_offer.clone(),
            fallible_unshielded_offer: self.fallible_unshielded_offer.clone(),
            actions: self
                .actions
                .clone()
                .iter()
                .map(|x| x.erase_proof())
                .collect(),
            dust_actions: self
                .dust_actions
                .as_ref()
                .map(|act| Sp::new(act.erase_proofs())),
            ttl: self.ttl,
            binding_commitment: self.binding_commitment.downgrade(),
        }
    }

    pub fn erase_signatures(&self) -> Intent<(), P, B, D>
    where
        UnshieldedOffer<S, D>: Clone,
    {
        Intent {
            guaranteed_unshielded_offer: self
                .guaranteed_unshielded_offer
                .clone()
                .map(|x| Sp::new(x.erase_signatures())),
            fallible_unshielded_offer: self
                .fallible_unshielded_offer
                .clone()
                .map(|x| Sp::new(x.erase_signatures())),
            actions: self.actions.clone(),
            dust_actions: self
                .dust_actions
                .as_ref()
                .map(|act| Sp::new(act.erase_signatures())),
            ttl: self.ttl,
            binding_commitment: self.binding_commitment.clone(),
        }
    }

    pub fn guaranteed_inputs(&self) -> Vec<UtxoSpend> {
        self.guaranteed_unshielded_offer
            .clone()
            .map(|guo| guo.inputs.clone())
            .unwrap_or_default()
            .into()
    }

    pub fn fallible_inputs(&self) -> Vec<UtxoSpend> {
        self.fallible_unshielded_offer
            .clone()
            .map(|fuo| fuo.inputs.clone())
            .unwrap_or_default()
            .into()
    }

    pub fn guaranteed_outputs(&self) -> Vec<UtxoOutput> {
        self.guaranteed_unshielded_offer
            .clone()
            .map(|guo| guo.outputs.clone())
            .unwrap_or_default()
            .into()
    }

    pub fn fallible_outputs(&self) -> Vec<UtxoOutput> {
        self.fallible_unshielded_offer
            .clone()
            .map(|fuo| fuo.outputs.clone())
            .unwrap_or_default()
            .into()
    }
}

impl<D: DB> Intent<(), (), Pedersen, D> {
    pub fn data_to_sign(&self, segment_id: u16) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend(b"midnight:hash-intent:");
        Serializable::serialize(&segment_id, &mut data)
            .expect("In-memory serialization should succeed");
        to_hash_data::<D>(self.clone(), data)
    }
}

#[derive(Storable)]
#[derive_where(Debug, Clone, PartialEq, Eq)]
#[storable(db = D)]
#[tag = "replay-protection-state[v1]"]
#[must_use]
pub struct ReplayProtectionState<D: DB> {
    pub time_filter_map: TimeFilterMap<HashSet<IntentHash, D>, D>,
}
tag_enforcement_test!(ReplayProtectionState<InMemoryDB>);

impl<D: DB> ReplayProtectionState<D> {
    pub fn new() -> ReplayProtectionState<D> {
        ReplayProtectionState {
            time_filter_map: TimeFilterMap::new(),
        }
    }
}

impl<D: DB> Default for ReplayProtectionState<D> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serializable, Serialize, Deserialize)]
#[tag = "transaction-cost-model[v4]"]
pub struct TransactionCostModel {
    pub runtime_cost_model: onchain_runtime::cost_model::CostModel,
    pub parallelism_factor: u64,
    pub baseline_cost: RunningCost,
}

impl TransactionCostModel {
    pub(crate) fn cell_read(&self, size: u64) -> RunningCost {
        self.runtime_cost_model.read_cell(size, true)
    }
    fn cell_write(&self, size: u64, overwrite: bool) -> RunningCost {
        RunningCost {
            bytes_written: size,
            bytes_deleted: if overwrite { size } else { 0 },
            ..RunningCost::ZERO
        }
    }
    fn cell_delete(&self, size: u64) -> RunningCost {
        RunningCost {
            bytes_deleted: size,
            ..RunningCost::ZERO
        }
    }
    pub(crate) fn map_index(&self, log_size: usize) -> RunningCost {
        self.runtime_cost_model.read_map(log_size, true)
    }
    pub(crate) fn proof_verify(&self, size: usize) -> RunningCost {
        let time = self.runtime_cost_model.proof_verify_constant
            + self.runtime_cost_model.proof_verify_coeff_size * size;
        RunningCost::compute(time)
    }
    fn map_insert(&self, log_size: usize, overwrite: bool) -> RunningCost {
        let layers = log_size.div_ceil(4);
        self.cell_write(PERSISTENT_HASH_BYTES as u64 * 16, true) * layers as u64
            + self.cell_write(PERSISTENT_HASH_BYTES as u64, overwrite)
    }
    fn map_remove(&self, log_size: usize, guaranteed_present: bool) -> RunningCost {
        if guaranteed_present {
            self.map_insert(log_size, true)
        } else {
            self.map_insert(log_size, true) + self.map_index(log_size)
        }
    }
    fn time_filter_map_lookup(&self) -> RunningCost {
        // TODO: This is a good approximation, but not accurate.
        self.map_index(8) * 2u64
    }
    fn time_filter_map_insert(&self, overwrite: bool) -> RunningCost {
        // Two map insertions, the 'set' and the 'time_map'.
        self.map_insert(8, overwrite) * 2u64
    }
    fn merkle_tree_index(&self, log_size: usize) -> RunningCost {
        self.runtime_cost_model.read_bmt(log_size, true)
    }
    fn merkle_tree_insert_no_rehash(&self, log_size: usize, overwrite: bool) -> RunningCost {
        // Cost all but the last layer as 'overwrites' -- this isn't strictly
        // true, but it captures the spirit here, in that these trees are sized
        // specifically for the protocol to *fill* them. Therefore, we
        // essentially consider everything except the leaf to already be
        // 'reserved'.
        let raw_writes = self.cell_write(FR_BYTES as u64, overwrite);
        let raw_overwrites = self.cell_write((FR_BYTES * 3 + 2) as u64, true) * log_size as u64;
        raw_writes + raw_overwrites
    }
    fn merkle_tree_insert_unamortized(&self, log_size: usize, overwrite: bool) -> RunningCost {
        let rehashes = RunningCost::compute(self.runtime_cost_model.transient_hash * log_size);
        rehashes + self.merkle_tree_insert_no_rehash(log_size, overwrite)
    }
    fn merkle_tree_insert_amortized(&self, log_size: usize, overwrite: bool) -> RunningCost {
        // The amortization turns n writes into a tree of depth d from
        // nd hashes to n log_2 n + d hashes. That means that the hashes we cost *per insert*
        // isn't constant.
        //
        // Assuming 16 insertions per rehash, and d=32, that's 16 * 4 + 32 = 96 total hashes, or
        // 6 hashes per insertion
        const EXPECTED_AMORTIZE_HASHES: usize = 6;
        let rehashes =
            RunningCost::compute(self.runtime_cost_model.transient_hash * EXPECTED_AMORTIZE_HASHES);
        rehashes + self.merkle_tree_insert_no_rehash(log_size, overwrite)
    }
    fn tree_copy<T: Storable<D>, D: DB>(&self, value: Sp<T, D>) -> RunningCost {
        self.runtime_cost_model.tree_copy(value)
    }
    fn stack_setup_cost<D: DB>(&self, transcript: &Transcript<D>) -> RunningCost {
        // There's an additional cost of processing a transcript coming from initializing effects
        // and context stack variables. This accounts for them.
        // The bulk of this is MPT insertions to build up various sets that sit in these values.
        // To unify these, we first capture a list of element numbers and element sizes, assuming
        // they are constant by container, and then reduce that to a running cost using map
        // insert VM operations.
        const EXPECTED_COM_INDICES: usize = 16;
        let eff = &transcript.effects;
        // (amount, key length)
        let maps = [
            (EXPECTED_COM_INDICES, PERSISTENT_HASH_BYTES),
            (eff.claimed_nullifiers.size(), PERSISTENT_HASH_BYTES),
            (eff.claimed_shielded_receives.size(), PERSISTENT_HASH_BYTES),
            (eff.claimed_shielded_spends.size(), PERSISTENT_HASH_BYTES),
            (
                eff.claimed_contract_calls.size(),
                PERSISTENT_HASH_BYTES * 2 + FR_BYTES + 8,
            ),
            (eff.shielded_mints.size(), PERSISTENT_HASH_BYTES),
            (eff.unshielded_mints.size(), PERSISTENT_HASH_BYTES),
            (eff.unshielded_inputs.size(), PERSISTENT_HASH_BYTES + 1),
            (eff.unshielded_outputs.size(), PERSISTENT_HASH_BYTES + 1),
            (
                eff.claimed_unshielded_spends.size(),
                PERSISTENT_HASH_BYTES * 2 + 1,
            ),
        ];
        let time = maps
            .iter()
            .map(|(n, key_len)| {
                self.runtime_cost_model.ins_map_constant
                    + self.runtime_cost_model.ins_map_coeff_container_log_size
                        * n.next_power_of_two().ilog2() as u64
                    + self.runtime_cost_model.ins_map_coeff_key_size * *key_len
            })
            .sum();
        RunningCost::compute(time)
    }
}
tag_enforcement_test!(TransactionCostModel);

pub const INITIAL_TRANSACTION_COST_MODEL: TransactionCostModel = TransactionCostModel {
    runtime_cost_model: onchain_runtime::cost_model::INITIAL_COST_MODEL,
    parallelism_factor: 4,
    baseline_cost: RunningCost {
        compute_time: CostDuration::from_picoseconds(100_000_000),
        read_time: CostDuration::ZERO,
        bytes_written: 0,
        bytes_deleted: 0,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, Serializable)]
#[cfg_attr(feature = "fixed-point-custom-serde", derive(Serialize, Deserialize))]
#[tag = "transaction-limits[v2]"]
pub struct TransactionLimits {
    pub transaction_byte_limit: u64,
    pub time_to_dismiss_per_byte: CostDuration,
    pub min_time_to_dismiss: CostDuration,
    pub block_limits: SyntheticCost,
    /// The minimum amount of Night withdrawable from block rewards, as a
    /// multiple of the amount which would be able to pay the theoretical fees
    /// for the withdrawal when Dust reaches its cap.
    #[cfg_attr(
        feature = "fixed-point-custom-serde",
        serde(with = "base_crypto::cost_model::fixed_point_custom_serde")
    )]
    pub block_withdrawal_minimum_multiple: FixedPoint,
}
tag_enforcement_test!(TransactionLimits);

pub const INITIAL_LIMITS: TransactionLimits = TransactionLimits {
    transaction_byte_limit: 1 << 20, // 1 MiB
    time_to_dismiss_per_byte: CostDuration::from_picoseconds(2_000_000),
    min_time_to_dismiss: CostDuration::from_picoseconds(15_000_000_000),
    block_limits: SyntheticCost {
        read_time: CostDuration::SECOND,
        compute_time: CostDuration::SECOND,
        block_usage: 200_000,
        bytes_written: 50_000,
        bytes_churned: 1_000_000,
    },
    block_withdrawal_minimum_multiple: FixedPoint::from_u64_div(1, 2),
};

#[derive(Clone, Debug, PartialEq, Eq, Serializable, Storable)]
#[cfg_attr(
    feature = "fixed-point-custom-serde",
    derive(serde::Serialize, serde::Deserialize)
)]
#[tag = "ledger-parameters[v5]"]
#[storable(base)]
pub struct LedgerParameters {
    pub cost_model: TransactionCostModel,
    pub limits: TransactionLimits,
    pub dust: DustParameters,
    pub fee_prices: FeePrices,
    pub global_ttl: Duration,
    // Valid range of 0..1
    #[cfg_attr(
        feature = "fixed-point-custom-serde",
        serde(with = "base_crypto::cost_model::fixed_point_custom_serde")
    )]
    pub cost_dimension_min_ratio: FixedPoint,
    #[cfg_attr(
        feature = "fixed-point-custom-serde",
        serde(with = "base_crypto::cost_model::fixed_point_custom_serde")
    )]
    pub price_adjustment_a_parameter: FixedPoint,
    // Note: This is equivalent to `c_to_m_bridge_fee_percent` in the spec
    // Valid range of 0..10_000
    pub cardano_to_midnight_bridge_fee_basis_points: u32,
    // Note: This is denominated in STARs (atomic night units)
    pub c_to_m_bridge_min_amount: u128,
}
tag_enforcement_test!(LedgerParameters);

impl LedgerParameters {
    /// The maximum price adjustment per block with the current parameters, as a multiplicative
    /// factor (that is: 1.1 would indicate a 10% adjustment). Will always return the positive (>1)
    /// adjustment factor. Note that negative adjustments are the additive inverse (1.1 has a
    /// corresponding 0.9 downward adjustment), *not* the multiplicative as might reasonably be
    /// assumed.
    pub fn max_price_adjustment(&self) -> FixedPoint {
        price_adjustment_function(FixedPoint::ONE, self.price_adjustment_a_parameter)
            + FixedPoint::ONE
    }

    pub fn min_claimable_rewards(&self) -> u128 {
        let Ok(synthetic_fee) =
            Transaction::ClaimRewards::<Signature, ProofMarker, PureGeneratorPedersen, InMemoryDB>(
                ClaimRewardsTransaction {
                    network_id: "phantom-value".into(),
                    value: u128::MAX,
                    owner: Default::default(),
                    nonce: Default::default(),
                    signature: Default::default(),
                    kind: ClaimKind::Reward,
                },
            )
            .cost(self, true)
        else {
            return u128::MAX;
        };
        let real_dust_fee = synthetic_fee
            .normalize(self.limits.block_limits)
            .map(|norm| self.fee_prices.overall_cost(&norm))
            .unwrap_or(FixedPoint::MAX);
        let night_dust_fp = FixedPoint::from_u64_div(
            self.dust.night_dust_ratio,
            (SPECKS_PER_DUST / STARS_PER_NIGHT) as u64,
        );
        let night_to_pay_at_cap = real_dust_fee / night_dust_fp;
        let min_night_fp = night_to_pay_at_cap * self.limits.block_withdrawal_minimum_multiple;
        min_night_fp.into_atomic_units(STARS_PER_NIGHT)
    }
}

pub const INITIAL_PARAMETERS: LedgerParameters = LedgerParameters {
    cost_model: INITIAL_TRANSACTION_COST_MODEL,
    limits: INITIAL_LIMITS,
    dust: INITIAL_DUST_PARAMETERS,
    fee_prices: FeePrices {
        overall_price: FixedPoint::from_u64_div(10, 1),
        read_factor: FixedPoint::ONE,
        compute_factor: FixedPoint::ONE,
        block_usage_factor: FixedPoint::ONE,
        write_factor: FixedPoint::ONE,
    },
    global_ttl: Duration::from_secs(3600),
    cardano_to_midnight_bridge_fee_basis_points: 500,
    cost_dimension_min_ratio: FixedPoint::from_u64_div(1, 4),
    price_adjustment_a_parameter: FixedPoint::from_u64_div(100, 1),
    c_to_m_bridge_min_amount: 1000,
};

#[derive(Storable)]
#[storable(db = D)]
#[derive_where(Clone; S, B, P)]
#[tag = "transaction[v9]"]
// TODO: Getting `Box` to serialize is a pain right now. Revisit later.
#[allow(clippy::large_enum_variant)]
pub enum Transaction<S: SignatureKind<D>, P: ProofKind<D>, B: Storable<D>, D: DB> {
    Standard(StandardTransaction<S, P, B, D>),
    ClaimRewards(ClaimRewardsTransaction<S, D>),
}
tag_enforcement_test!(Transaction<(), (), Pedersen, InMemoryDB>);

#[derive_where(Debug, Clone)]
pub struct VerifiedTransaction<D: DB> {
    pub(crate) inner: Transaction<(), (), Pedersen, D>,
    pub(crate) hash: TransactionHash,
}

impl<D: DB> Deref for VerifiedTransaction<D> {
    type Target = Transaction<(), (), Pedersen, D>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<S: SignatureKind<D> + Debug, P: ProofKind<D> + Debug, B: Storable<D> + Debug, D: DB> Debug
    for Transaction<S, P, B, D>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Transaction::Standard(stx) => stx.fmt(f),
            Transaction::ClaimRewards(cm) => cm.fmt(f),
        }
    }
}

impl<
    S: SignatureKind<D>,
    P: ProofKind<D>,
    B: Storable<D> + PedersenDowngradeable<D> + Serializable,
    D: DB,
> Transaction<S, P, B, D>
{
    pub fn erase_proofs(&self) -> Transaction<S, (), Pedersen, D> {
        match self {
            Transaction::Standard(StandardTransaction {
                network_id,
                intents,
                guaranteed_coins,
                fallible_coins,
                binding_randomness,
                ..
            }) => Transaction::Standard(StandardTransaction {
                network_id: network_id.clone(),
                intents: intents
                    .iter()
                    .map(|seg_intent| {
                        (
                            *seg_intent.deref().0.deref(),
                            seg_intent.deref().1.deref().erase_proofs(),
                        )
                    })
                    .collect(),
                guaranteed_coins: guaranteed_coins
                    .as_ref()
                    .map(|x| Sp::new(ZswapOffer::erase_proofs(x.deref()))),
                fallible_coins: fallible_coins
                    .iter()
                    .map(|sp| (*sp.deref().0.deref(), sp.deref().1.deref().erase_proofs()))
                    .collect(),
                binding_randomness: *binding_randomness,
            }),
            Transaction::ClaimRewards(claim) => Transaction::ClaimRewards(claim.clone()),
        }
    }

    pub fn erase_signatures(&self) -> Transaction<(), P, B, D> {
        match self {
            Transaction::Standard(StandardTransaction {
                network_id,
                intents,
                guaranteed_coins,
                fallible_coins,
                binding_randomness,
                ..
            }) => Transaction::Standard(StandardTransaction {
                network_id: network_id.clone(),
                intents: intents
                    .iter()
                    .map(|sp| {
                        (
                            *sp.deref().0.deref(),
                            sp.deref().1.deref().erase_signatures(),
                        )
                    })
                    .collect(),
                guaranteed_coins: guaranteed_coins.clone(),
                fallible_coins: fallible_coins.clone(),
                binding_randomness: *binding_randomness,
            }),
            Transaction::ClaimRewards(claim) => Transaction::ClaimRewards(claim.erase_signatures()),
        }
    }

    #[instrument(skip(self, other))]
    pub fn merge(&self, other: &Self) -> Result<Self, MalformedTransaction<D>> {
        use Transaction as T;
        match (self, other) {
            (T::Standard(stx1), T::Standard(stx2)) => {
                if stx1.network_id != stx2.network_id {
                    return Err(MalformedTransaction::InvalidNetworkId {
                        expected: stx1.network_id.clone(),
                        found: stx2.network_id.clone(),
                    });
                }
                let res = Transaction::Standard(StandardTransaction {
                    network_id: stx1.network_id.clone(),
                    intents: stx1
                        .intents
                        .clone()
                        .into_iter()
                        .chain(stx2.intents.clone())
                        // Would be nicer to add Entry for HashMap and use that
                        .try_fold(HashMap::new(), |mut acc, (k, v)| {
                            if acc.contains_key(&k) {
                                Err(MalformedTransaction::<D>::IntentSegmentIdCollision(k))
                            } else {
                                acc = acc.insert(k, v);
                                Ok(acc)
                            }
                        })?,
                    guaranteed_coins: match (
                        stx1.guaranteed_coins.as_ref(),
                        stx2.guaranteed_coins.as_ref(),
                    ) {
                        (Some(gc1), Some(gc2)) => Some(Sp::new(
                            gc1.merge(gc2).map_err(MalformedTransaction::<D>::Zswap)?,
                        )),
                        (Some(gc1), None) => Some(gc1.clone()),
                        (None, Some(gc2)) => Some(gc2.clone()),
                        (None, None) => None,
                    },
                    fallible_coins: {
                        let mut result: std::collections::BTreeMap<
                            u16,
                            ZswapOffer<P::LatestProof, D>,
                        > = stx1.fallible_coins.clone().into_iter().collect();
                        for (key, offer2) in stx2.fallible_coins.clone() {
                            match result.get(&key) {
                                Some(offer1) => {
                                    result.insert(key, offer1.merge(&offer2)?);
                                }
                                None => {
                                    result.insert(key, offer2.clone());
                                }
                            }
                        }

                        result.into_iter().collect()
                    },
                    binding_randomness: stx1.binding_randomness + stx2.binding_randomness,
                });
                debug!("transaction merged");
                Ok(res)
            }
            _ => Err(MalformedTransaction::<D>::CantMergeTypes),
        }
    }

    pub fn identifiers(&'_ self) -> impl Iterator<Item = TransactionIdentifier> + '_ {
        let mut res = Vec::new();
        match self {
            Transaction::Standard(stx) => res.extend(
                stx.inputs()
                    .map(|i| i.value_commitment)
                    .chain(stx.outputs().map(|o| o.value_commitment))
                    .chain(stx.transients().map(|io| io.value_commitment_input))
                    .chain(stx.transients().map(|io| io.value_commitment_output))
                    .chain(
                        stx.intents()
                            .map(|(_, intent)| intent.binding_commitment.downgrade()),
                    )
                    .map(TransactionIdentifier::Merged),
            ),
            Transaction::ClaimRewards(claim) => res.push(TransactionIdentifier::Unique(
                OutputInstructionUnshielded {
                    amount: claim.value,
                    target_address: claim.owner.clone().into(),
                    nonce: claim.nonce,
                }
                .clone()
                .mk_intent_hash(NIGHT)
                .0,
            )),
        }
        res.into_iter()
    }

    pub fn actions(&self) -> impl Iterator<Item = (u16, ContractAction<P, D>)> {
        match self {
            Transaction::Standard(stx) => stx.actions().collect(),
            _ => Vec::new(),
        }
        .into_iter()
    }

    pub fn deploys(&self) -> impl Iterator<Item = (u16, ContractDeploy<D>)> {
        match self {
            Transaction::Standard(stx) => stx.deploys().collect(),
            _ => Vec::new(),
        }
        .into_iter()
    }

    pub fn updates(&'_ self) -> impl Iterator<Item = (u16, MaintenanceUpdate<D>)> {
        match self {
            Transaction::Standard(stx) => stx.updates().collect(),
            _ => Vec::new(),
        }
        .into_iter()
    }

    pub fn calls(&'_ self) -> impl Iterator<Item = (u16, ContractCall<P, D>)> {
        match self {
            Transaction::Standard(stx) => stx.calls().collect(),
            _ => Vec::new(),
        }
        .into_iter()
    }

    pub fn intents(&'_ self) -> impl Iterator<Item = (u16, Intent<S, P, B, D>)> {
        match self {
            Transaction::Standard(stx) => stx.intents().collect(),
            _ => Vec::new(),
        }
        .into_iter()
    }

    pub fn has_identifier(&self, ident: &TransactionIdentifier) -> bool {
        self.identifiers().any(|ident2| ident == &ident2)
    }
}

impl<S: SignatureKind<D>, P: ProofKind<D>, B: Storable<D>, D: DB> Transaction<S, P, B, D>
where
    Transaction<S, P, B, D>: Serializable,
{
    pub fn segments(&self) -> Vec<u16> {
        match self {
            Self::Standard(stx) => stx.segments(),
            Self::ClaimRewards(_) => Vec::new(),
        }
    }
}

impl<S: SignatureKind<D>, P: ProofKind<D>, B: Storable<D>, D: DB> Intent<S, P, B, D> {
    pub fn calls(&self) -> impl Iterator<Item = &ContractCall<P, D>> {
        self.actions.iter_deref().filter_map(|cd| match cd {
            ContractAction::Call(upd) => Some(&**upd),
            _ => None,
        })
    }

    pub fn calls_owned(&self) -> Vec<ContractCall<P, D>>
    where
        ContractCall<P, D>: Clone,
    {
        self.actions
            .iter_deref()
            .filter_map(|cd| match cd {
                ContractAction::Call(upd) => Some((**upd).clone()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Storable)]
#[storable(db = D)]
#[derive_where(Clone, Debug; S, P, B)]
#[tag = "standard-transaction[v9]"]
pub struct StandardTransaction<S: SignatureKind<D>, P: ProofKind<D>, B: Storable<D>, D: DB> {
    pub network_id: String,
    pub intents: HashMap<u16, Intent<S, P, B, D>, D>,
    pub guaranteed_coins: Option<Sp<ZswapOffer<P::LatestProof, D>, D>>,
    pub fallible_coins: HashMap<u16, ZswapOffer<P::LatestProof, D>, D>,
    pub binding_randomness: PedersenRandomness,
}
tag_enforcement_test!(StandardTransaction<(), (), Pedersen, InMemoryDB>);

impl<S: SignatureKind<D>, P: ProofKind<D> + Serializable + Deserializable, B: Storable<D>, D: DB>
    StandardTransaction<S, P, B, D>
{
    pub fn actions(&self) -> impl Iterator<Item = (u16, ContractAction<P, D>)> {
        self.intents
            .clone()
            .into_iter()
            .flat_map(|(segment_id, intent)| {
                Vec::from(&intent.actions)
                    .into_iter()
                    .map(move |act| (segment_id, act))
            })
    }

    pub fn deploys(&self) -> impl Iterator<Item = (u16, ContractDeploy<D>)> {
        self.intents
            .clone()
            .into_iter()
            .flat_map(|(segment_id, intent)| {
                Vec::from(&intent.actions)
                    .into_iter()
                    .map(move |act| (segment_id, act))
            })
            .filter_map(|(segment_id, action)| match action {
                ContractAction::Deploy(d) => Some((segment_id, (*d).clone())),
                _ => None,
            })
    }

    pub fn updates(&self) -> impl Iterator<Item = (u16, MaintenanceUpdate<D>)> {
        self.intents
            .clone()
            .into_iter()
            .flat_map(|(segment_id, intent)| {
                Vec::from(&intent.actions)
                    .into_iter()
                    .map(move |act| (segment_id, act))
            })
            .filter_map(|(segment_id, action)| match action {
                ContractAction::Maintain(upd) => Some((segment_id, upd)),
                _ => None,
            })
    }

    pub fn calls(&self) -> impl Iterator<Item = (u16, ContractCall<P, D>)> {
        self.intents
            .clone()
            .into_iter()
            .flat_map(|(segment_id, intent)| {
                Vec::from(&intent.actions)
                    .into_iter()
                    .filter_map(move |action| {
                        if let ContractAction::Call(upd) = action {
                            Some((segment_id, (*upd).clone()))
                        } else {
                            None
                        }
                    })
            })
    }

    pub fn intents(&'_ self) -> impl Iterator<Item = (u16, Intent<S, P, B, D>)> {
        self.intents.clone().into_iter()
    }

    pub fn inputs(
        &self,
    ) -> impl Iterator<Item = Input<<P as ProofKind<D>>::LatestProof, D>> + use<'_, S, P, B, D>
    {
        self.guaranteed_inputs().chain(self.fallible_inputs())
    }

    pub fn guaranteed_inputs(
        &self,
    ) -> impl Iterator<Item = Input<<P as ProofKind<D>>::LatestProof, D>> + use<'_, S, P, B, D>
    {
        self.guaranteed_coins
            .iter()
            .flat_map(|gc| Vec::from(&gc.inputs).into_iter())
    }

    pub fn fallible_inputs(
        &self,
    ) -> impl Iterator<Item = Input<<P as ProofKind<D>>::LatestProof, D>> {
        self.fallible_coins
            .iter()
            .flat_map(|sp| Vec::from(&sp.1.inputs).into_iter())
    }

    pub fn outputs(
        &self,
    ) -> impl Iterator<Item = Output<<P as ProofKind<D>>::LatestProof, D>> + use<'_, S, P, B, D>
    {
        self.guaranteed_outputs().chain(self.fallible_outputs())
    }

    pub fn guaranteed_outputs(
        &self,
    ) -> impl Iterator<Item = Output<<P as ProofKind<D>>::LatestProof, D>> + use<'_, S, P, B, D>
    {
        self.guaranteed_coins
            .iter()
            .flat_map(|gc| Vec::from(&gc.outputs).into_iter())
    }

    pub fn fallible_outputs(
        &'_ self,
    ) -> impl Iterator<Item = Output<<P as ProofKind<D>>::LatestProof, D>> {
        self.fallible_coins
            .iter()
            .flat_map(|sp| Vec::from(&sp.1.outputs).into_iter())
    }

    pub fn transients(
        &self,
    ) -> impl Iterator<Item = Transient<<P as ProofKind<D>>::LatestProof, D>> + use<'_, S, P, B, D>
    {
        self.guaranteed_transients()
            .chain(self.fallible_transients())
    }

    pub fn guaranteed_transients(
        &self,
    ) -> impl Iterator<Item = Transient<<P as ProofKind<D>>::LatestProof, D>> + use<'_, S, P, B, D>
    {
        self.guaranteed_coins
            .iter()
            .flat_map(|offer| Vec::from(&offer.transient).into_iter())
    }

    pub fn fallible_transients(
        &self,
    ) -> impl Iterator<Item = Transient<<P as ProofKind<D>>::LatestProof, D>> + use<'_, S, P, B, D>
    {
        self.fallible_coins
            .iter()
            .flat_map(|offer| Vec::from(&offer.1.transient).into_iter())
    }

    pub fn segments(&self) -> Vec<u16> {
        let mut segments = once(0)
            .chain(self.intents.iter().map(|seg_intent| *seg_intent.0))
            .chain(self.fallible_coins.iter().map(|seg_offer| *seg_offer.0))
            .collect::<Vec<_>>();
        segments.sort();
        segments.dedup();
        segments
    }
}

type ErasedClaimRewardsTransaction<D> = ClaimRewardsTransaction<(), D>;

#[derive(Storable)]
#[derive_where(Clone, PartialEq, Eq; S)]
#[storable(db = D)]
#[tag = "claim-rewards-transaction[v1]"]
pub struct ClaimRewardsTransaction<S: SignatureKind<D>, D: DB> {
    pub network_id: String,
    pub value: u128,
    pub owner: VerifyingKey,
    pub nonce: Nonce,
    pub signature: S::Signature<ErasedClaimRewardsTransaction<D>>,
    pub kind: ClaimKind,
}
tag_enforcement_test!(ClaimRewardsTransaction<(), InMemoryDB>);

impl<S: SignatureKind<D>, D: DB> ClaimRewardsTransaction<S, D> {
    pub fn add_signature(&self, signature: Signature) -> ClaimRewardsTransaction<Signature, D> {
        ClaimRewardsTransaction {
            network_id: self.network_id.clone(),
            value: self.value,
            owner: self.owner.clone(),
            nonce: self.nonce,
            signature,
            kind: self.kind,
        }
    }

    pub fn erase_signatures(&self) -> ErasedClaimRewardsTransaction<D> {
        ClaimRewardsTransaction {
            network_id: self.network_id.clone(),
            value: self.value,
            owner: self.owner.clone(),
            nonce: self.nonce,
            signature: (),
            kind: self.kind,
        }
    }
}

impl<D: DB> ClaimRewardsTransaction<(), D> {
    pub fn data_to_sign(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend(b"midnight:sig-claim_rewards_transaction:");
        Self::to_hash_data(self.clone(), data)
    }

    pub fn to_hash_data(rewards: ClaimRewardsTransaction<(), D>, mut data: Vec<u8>) -> Vec<u8> {
        Serializable::serialize(&rewards.value, &mut data)
            .expect("In-memory serialization should succeed");
        Serializable::serialize(&rewards.owner, &mut data)
            .expect("In-memory serialization should succeed");
        Serializable::serialize(&rewards.nonce, &mut data)
            .expect("In-memory serialization should succeed");
        data
    }
}

impl<S: SignatureKind<D>, D: DB> Debug for ClaimRewardsTransaction<S, D> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(
            formatter,
            "<rewards of {} of Night for recipient {:?}>",
            self.value, self.owner
        )
    }
}

pub(crate) const MIN_PROOF_SIZE: usize = DUST_SPEND_PROOF_SIZE;
pub(crate) const PROOF_SIZE: usize = zswap::INPUT_PROOF_SIZE;
// Retrieved from zswap key size. Unfortunately varies with circuits, this
// should be an upper bound.
pub(crate) const VERIFIER_KEY_SIZE: usize = 2875;
pub(crate) const EXPECTED_CONTRACT_DEPTH: usize = 32;
pub(crate) const EXPECTED_UTXO_DEPTH: usize = 32;
pub(crate) const EXPECTED_GENERATION_DEPTH: usize = 32;
pub(crate) const EXPECTED_OPERATIONS_DEPTH: usize = 8;
pub(crate) const EXPECTED_TOKEN_TYPE_DEPTH: usize = 8;
pub(crate) const EXPECTED_TIME_FILTER_MAP_DEPTH: usize = 8;
pub(crate) const FRESH_DUST_COMMITMENT_HASHES: usize = 6;

impl<
    S: SignatureKind<D>,
    P: ProofKind<D>,
    B: Storable<D> + PedersenDowngradeable<D> + Serializable,
    D: DB,
> Transaction<S, P, B, D>
where
    Transaction<S, P, B, D>: Serializable,
{
    pub fn fees_with_margin(
        &self,
        params: &LedgerParameters,
        margin: usize,
    ) -> Result<u128, FeeCalculationError> {
        let synthetic = self.cost(params, false)?;
        let normalized = synthetic
            .normalize(params.limits.block_limits)
            .ok_or(FeeCalculationError::BlockLimitExceeded)?;
        let fees_fixed_point = params.fee_prices.overall_cost(&normalized);
        let margin_fees = fees_fixed_point * params.max_price_adjustment().powi(margin as i32);
        Ok(margin_fees.into_atomic_units(SPECKS_PER_DUST))
    }

    pub fn fees(
        &self,
        params: &LedgerParameters,
        enforce_time_to_dismiss: bool,
    ) -> Result<u128, FeeCalculationError> {
        let synthetic = self.cost(params, enforce_time_to_dismiss)?;
        let normalized = synthetic
            .normalize(params.limits.block_limits)
            .ok_or(FeeCalculationError::BlockLimitExceeded)?;
        let fees_fixed_point = params.fee_prices.overall_cost(&normalized);
        Ok(fees_fixed_point.into_atomic_units(SPECKS_PER_DUST))
    }

    pub fn validation_cost(&self, model: &TransactionCostModel) -> SyntheticCost {
        match self {
            Transaction::Standard(stx) => {
                let vk_reads = self
                    .calls()
                    .map(|(_, call)| (call.address, call.entry_point))
                    .collect::<BTreeSet<_>>()
                    .len();
                let mut cost = model.baseline_cost;
                cost += (model.cell_read(VERIFIER_KEY_SIZE as u64)
                    + model.map_index(EXPECTED_CONTRACT_DEPTH)
                    + model.map_index(EXPECTED_OPERATIONS_DEPTH))
                    * vk_reads;
                let offers = stx
                    .guaranteed_coins
                    .iter()
                    .map(|o| (**o).clone())
                    .chain(stx.fallible_coins.values())
                    .collect::<Vec<_>>();
                let zswap_inputs = offers
                    .iter()
                    .map(|offer| offer.inputs.len() + offer.transient.len())
                    .sum::<usize>();
                let zswap_outputs = offers
                    .iter()
                    .map(|offer| offer.outputs.len() + offer.transient.len())
                    .sum::<usize>();
                cost += model.proof_verify(zswap::INPUT_PIS) * zswap_inputs;
                cost += model.proof_verify(zswap::OUTPUT_PIS) * zswap_outputs;
                for intent in stx.intents.values() {
                    // Binding commitment check
                    cost.compute_time += model.runtime_cost_model.pedersen_valid;
                    // Unshielded offer validation
                    cost.compute_time += intent
                        .guaranteed_unshielded_offer
                        .iter()
                        .chain(intent.fallible_unshielded_offer.iter())
                        .map(|o| o.signatures.len())
                        .sum::<usize>()
                        * model.runtime_cost_model.signature_verify_constant;
                    for action in intent.actions.iter() {
                        match &*action {
                            ContractAction::Call(call) => {
                                cost.compute_time += model.runtime_cost_model.verifier_key_load;
                                cost += model
                                    .proof_verify(call.public_inputs(Default::default()).len());
                            }
                            ContractAction::Maintain(upd) => {
                                cost.compute_time +=
                                    model.runtime_cost_model.signature_verify_constant
                                        * upd.signatures.len();
                            }
                            _ => {}
                        }
                    }
                    if let Some(dust_actions) = intent.dust_actions {
                        cost += model.proof_verify(DUST_SPEND_PIS) * dust_actions.spends.len();
                        cost.compute_time += model.runtime_cost_model.signature_verify_constant
                            * dust_actions.registrations.len();
                    }
                }
                // Compute time for Pedersen check
                cost.compute_time += offers.iter().map(|o| o.deltas.len()).sum::<usize>()
                    * (model.runtime_cost_model.hash_to_curve + model.runtime_cost_model.ec_mul);
                cost.compute_time += model.runtime_cost_model.ec_mul;
                let mut res = SyntheticCost::from(cost);
                res.block_usage = self.est_size() as u64;
                res
            }
            Transaction::ClaimRewards(_) => (RunningCost {
                compute_time: model.runtime_cost_model.signature_verify_constant,
                ..RunningCost::ZERO
            } + model.baseline_cost)
                .into(),
        }
    }

    fn est_size(&self) -> usize {
        P::estimated_tx_size(self)
    }

    pub fn application_cost(&self, model: &TransactionCostModel) -> (SyntheticCost, SyntheticCost) {
        let mut g_cost = model.baseline_cost;
        let mut f_cost = RunningCost::ZERO;
        for (_, intent) in self.intents() {
            // Replay protection state update
            g_cost +=
                model.time_filter_map_lookup() + model.cell_read(PERSISTENT_HASH_BYTES as u64);
            g_cost += model.time_filter_map_insert(false)
                + model.cell_write(PERSISTENT_HASH_BYTES as u64, false);
            // utxo processing
            for (cost, offer) in [
                (&mut g_cost, intent.guaranteed_unshielded_offer.as_ref()),
                (&mut f_cost, intent.fallible_unshielded_offer.as_ref()),
            ]
            .into_iter()
            .filter_map(|(c, o)| o.map(|o| (c, o)))
            {
                let inputs = offer.inputs.len();
                let outputs = offer.outputs.len();
                // UTXO membership test
                *cost += model.map_index(EXPECTED_UTXO_DEPTH) * inputs;
                // UTXO removal
                *cost += (model.map_remove(EXPECTED_UTXO_DEPTH, true)
                    + model.cell_delete(UTXO_SIZE as u64))
                    * inputs;
                // UTXO insertion
                *cost += (model.map_insert(EXPECTED_UTXO_DEPTH, false)
                    + model.cell_write(UTXO_SIZE as u64, false))
                    * outputs;
                let night_inputs = offer.inputs.iter().filter(|i| i.type_ == NIGHT).count();
                let night_outputs = offer.outputs.iter().filter(|o| o.type_ == NIGHT).count();
                // Generating dtime update
                *cost += (model.merkle_tree_insert_unamortized(32, false)
                    + model.cell_write(DUST_GENERATION_INFO_SIZE as u64, false))
                    * night_inputs;
                // Night generates Dust address table read
                *cost += (model.map_index(EXPECTED_UTXO_DEPTH) + model.cell_read(FR_BYTES as u64))
                    * night_outputs;
                // Generation tree insertion & first-free update
                *cost += (model.cell_read(8) + model.cell_write(8, true)) * night_outputs;
                *cost += (model.merkle_tree_insert_amortized(32, false)
                    + model.cell_write(DUST_GENERATION_INFO_SIZE as u64, false))
                    * night_outputs;
                // Night indicies insertion
                *cost += (model.map_insert(EXPECTED_UTXO_DEPTH, false)
                    + model.cell_write(8, false))
                    * night_outputs;
                // Commitment merkle tree insertion & first-free update
                *cost += (model.cell_read(8) + model.cell_write(8, true)) * night_outputs;
                *cost += (model.merkle_tree_insert_amortized(EXPECTED_UTXO_DEPTH, false)
                    + model.cell_write(FR_BYTES as u64, false))
                    * night_outputs;
                // Commitment computation
                *cost += RunningCost::compute(
                    model.runtime_cost_model.transient_hash
                        * FRESH_DUST_COMMITMENT_HASHES
                        * night_outputs,
                );
            }
            let dust_spends = intent
                .dust_actions
                .as_ref()
                .map(|a| a.spends.len())
                .unwrap_or(0);
            // Nullifier membership test
            g_cost += model.map_index(32) * dust_spends;
            // Nullifier set insertion
            g_cost += (model.map_insert(EXPECTED_UTXO_DEPTH, false)
                + model.cell_write(FR_BYTES as u64, false))
                * dust_spends;
            // Commitment merkle tree insertion & first-free update
            g_cost += (model.cell_read(8) + model.cell_write(8, true)) * dust_spends;
            g_cost += (model.merkle_tree_insert_amortized(EXPECTED_UTXO_DEPTH, false)
                + model.cell_write(FR_BYTES as u64, false))
                * dust_spends;
            // Dust Merkle roots lookup
            g_cost += model.time_filter_map_lookup() * 2u64;
            for reg in intent
                .dust_actions
                .iter()
                .flat_map(|a| a.registrations.iter())
            {
                // Update the dust address registration table
                if reg.dust_address.is_some() {
                    g_cost += model.map_insert(EXPECTED_GENERATION_DEPTH, false)
                        + model.cell_write(FR_BYTES as u64, false);
                } else {
                    g_cost += model.map_remove(EXPECTED_GENERATION_DEPTH, true)
                        + model.cell_delete(FR_BYTES as u64);
                }
                // For each guaranteed night input with a matching address in
                // the intent, we read its ctime, and check it in the
                // generation indicies table.
                if reg.dust_address.is_some() {
                    let night_inputs = intent
                        .guaranteed_unshielded_offer
                        .iter()
                        .flat_map(|o| o.inputs.iter())
                        .filter(|i| i.owner == reg.night_key && i.type_ == NIGHT)
                        .count();
                    // Night indicies set check
                    g_cost +=
                        (model.map_index(EXPECTED_UTXO_DEPTH) + model.cell_read(8)) * night_inputs;
                    // Generation tree index
                    g_cost += (model.merkle_tree_index(EXPECTED_UTXO_DEPTH)
                        + model.cell_read(DUST_GENERATION_INFO_SIZE as u64))
                        * night_inputs;
                }
            }
            // Contract actions
            for action in intent.actions.iter() {
                match &*action {
                    ContractAction::Call(call) => {
                        let base_cost = if call.guaranteed_transcript.is_some() {
                            &mut g_cost
                        } else {
                            &mut f_cost
                        };
                        // Fetch / store state
                        *base_cost += model.map_index(EXPECTED_CONTRACT_DEPTH)
                            + model.map_insert(EXPECTED_CONTRACT_DEPTH, true)
                            + model.map_index(1)
                            + model.map_insert(1, true);
                        // Declared transcript costs
                        //
                        // NOTE: This is taken at face-value here. During
                        // execution, the declared cost (A `RunningCost`) is
                        // checked against the real cost at each operation, and
                        // aborted if it exceeds it (with the exception of
                        // `bytes_deleted`, which is checked after completion,
                        // and must be *at least* as declared).
                        for (cost, transcript) in [
                            (&mut g_cost, call.guaranteed_transcript.as_ref()),
                            (&mut f_cost, call.fallible_transcript.as_ref()),
                        ]
                        .into_iter()
                        .filter_map(|(c, t)| t.map(|t| (c, t)))
                        {
                            *cost += transcript.gas;
                            // VM stack setup / destroy cost
                            // Left out of scope here to avoid going to deep into
                            // stack structure.
                            *cost += model.stack_setup_cost(transcript);
                        }
                    }
                    ContractAction::Deploy(deploy) => {
                        // Contract exists check
                        f_cost += model.map_index(EXPECTED_CONTRACT_DEPTH);
                        // Contract insert
                        f_cost += model.map_insert(EXPECTED_CONTRACT_DEPTH, false)
                            + model.tree_copy(Sp::new(deploy.initial_state.clone()));
                    }
                    ContractAction::Maintain(upd) => {
                        // Contract state fetch
                        f_cost += model.map_index(EXPECTED_CONTRACT_DEPTH);
                        // Maintainance update counter update
                        f_cost += model.map_index(1) * 2u64
                            + model.map_insert(1, true) * 2u64
                            + model.cell_read(8)
                            + model.cell_write(8, true);
                        // Carrying out the updates
                        for part in upd.updates.iter() {
                            match &*part {
                                SingleUpdate::ReplaceAuthority(auth) => {
                                    f_cost += model.tree_copy::<_, D>(Sp::new(auth.clone()))
                                        + model.map_insert(1, true)
                                }
                                SingleUpdate::VerifierKeyRemove(..) => {
                                    f_cost += model.map_remove(EXPECTED_OPERATIONS_DEPTH, true)
                                        + model.cell_delete(VERIFIER_KEY_SIZE as u64)
                                        + model.map_insert(1, true)
                                }
                                SingleUpdate::VerifierKeyInsert(..) => {
                                    f_cost += model.map_insert(EXPECTED_OPERATIONS_DEPTH, false)
                                        + model.cell_write(VERIFIER_KEY_SIZE as u64, false)
                                        + model.map_insert(1, true)
                                }
                            }
                        }
                        // Inserting the new state
                        f_cost += model.map_insert(EXPECTED_CONTRACT_DEPTH, true);
                    }
                }
            }
        }
        match self {
            Transaction::Standard(stx) => {
                let offers = stx
                    .guaranteed_coins
                    .iter()
                    .map(|o| (0, (**o).clone()))
                    .chain(
                        stx.fallible_coins
                            .iter()
                            .map(|pair| (*pair.0, (*pair.1).clone())),
                    );
                for (segment, offer) in offers {
                    let mut offer_cost = RunningCost::ZERO;
                    let inputs = offer.inputs.len() + offer.transient.len();
                    let outputs = offer.outputs.len() + offer.transient.len();
                    // Roots set test
                    offer_cost += model.map_index(EXPECTED_TIME_FILTER_MAP_DEPTH) * inputs;
                    // Nullifier set test
                    offer_cost += model.map_index(EXPECTED_UTXO_DEPTH) * inputs;
                    // Nullifier set insertion
                    offer_cost += (model.map_insert(EXPECTED_UTXO_DEPTH, false)
                        + model.cell_write(PERSISTENT_HASH_BYTES as u64, false))
                        * inputs;
                    // Commitment set test
                    offer_cost += model.map_index(EXPECTED_UTXO_DEPTH) * outputs;
                    // Commitment set insertion
                    offer_cost += model.map_insert(EXPECTED_UTXO_DEPTH, false) * outputs;
                    // First free update
                    offer_cost += (model.cell_read(8) + model.cell_write(8, true)) * outputs;
                    // Merkle tree insertion
                    offer_cost +=
                        model.merkle_tree_insert_amortized(EXPECTED_UTXO_DEPTH, false) * outputs;
                    if segment == 0 {
                        g_cost += offer_cost;
                    } else {
                        f_cost += offer_cost;
                        // Because we also try to apply it in the guaranteed segment.
                        g_cost.compute_time += offer_cost.compute_time;
                    }
                }
            }
            Transaction::ClaimRewards(_) => {
                // Claim check
                g_cost += model.map_index(EXPECTED_UTXO_DEPTH);
                // Claim update
                g_cost += model.map_insert(EXPECTED_UTXO_DEPTH, true);
                // Replay protection update
                g_cost +=
                    model.time_filter_map_lookup() + model.cell_read(PERSISTENT_HASH_BYTES as u64);
                g_cost += model.time_filter_map_insert(false)
                    + model.cell_write(PERSISTENT_HASH_BYTES as u64, false);
                // Utxo update
                g_cost += model.map_insert(EXPECTED_UTXO_DEPTH, false);
            }
        }
        (g_cost.into(), (g_cost + f_cost).into())
    }

    pub fn time_to_dismiss(&self, model: &TransactionCostModel) -> CostDuration {
        let mut validation_cost = self.validation_cost(model);
        validation_cost.compute_time = validation_cost.compute_time / model.parallelism_factor;
        let guaranteed_cost = self.application_cost(model).0;
        let cost_to_dismiss = guaranteed_cost + validation_cost;
        CostDuration::max(cost_to_dismiss.compute_time, cost_to_dismiss.read_time)
    }

    pub fn cost(
        &self,
        params: &LedgerParameters,
        enforce_time_to_dismiss: bool,
    ) -> Result<SyntheticCost, FeeCalculationError> {
        let mut validation_cost = self.validation_cost(&params.cost_model);
        validation_cost.compute_time =
            validation_cost.compute_time / params.cost_model.parallelism_factor;
        let (guaranteed_cost, application_cost) = self.application_cost(&params.cost_model);
        let cost_to_dismiss = guaranteed_cost + validation_cost;
        let time_to_dismiss = CostDuration::max(
            params.limits.time_to_dismiss_per_byte * self.est_size() as u64,
            params.limits.min_time_to_dismiss,
        );
        if enforce_time_to_dismiss && cost_to_dismiss.max_time() > time_to_dismiss {
            return Err(FeeCalculationError::OutsideTimeToDismiss {
                time_to_dismiss: cost_to_dismiss.max_time(),
                allowed_time_to_dismiss: time_to_dismiss,
                size: self.est_size() as u64,
            });
        }
        Ok(validation_cost + application_cost)
    }
}

impl<S: SignatureKind<D>, P: ProofKind<D>, B: Serializable + Tagged + Storable<D>, D: DB>
    Transaction<S, P, B, D>
{
    pub fn transaction_hash(&self) -> TransactionHash {
        let mut hasher = Sha256::new();
        tagged_serialize(self, &mut hasher).expect("In-memory serialization must succeed");
        TransactionHash(HashOutput(hasher.finalize().into()))
    }
}

impl SystemTransaction {
    pub fn transaction_hash(&self) -> TransactionHash {
        let mut hasher = Sha256::new();
        tagged_serialize(self, &mut hasher).expect("In-memory serialization must succeed");
        TransactionHash(HashOutput(hasher.finalize().into()))
    }

    pub fn cost(&self, params: &LedgerParameters) -> SyntheticCost {
        use SystemTransaction::*;
        let model = &params.cost_model;
        match self {
            OverwriteParameters(new_params) => params
                .cost_model
                .cell_write(new_params.serialized_size() as u64, true),
            DistributeNight(_, outputs) => {
                let mut cost = RunningCost::ZERO;
                // Replay protection state update
                cost +=
                    model.time_filter_map_lookup() + model.cell_read(PERSISTENT_HASH_BYTES as u64);
                cost += model.time_filter_map_insert(false)
                    + model.cell_write(PERSISTENT_HASH_BYTES as u64, false);
                // map insertion, either bridge_receiving or unclaimed_block_rewards
                cost += model.map_insert(EXPECTED_GENERATION_DEPTH, false);
                cost += model.cell_write(16, false);
                // Commitment merkle tree insertion & first-free update
                cost += model.cell_read(8) + model.cell_write(8, true);
                cost += model.merkle_tree_insert_amortized(EXPECTED_UTXO_DEPTH, false)
                    + model.cell_write(FR_BYTES as u64, false);
                // Commitment computation
                cost += RunningCost::compute(
                    model.runtime_cost_model.transient_hash * FRESH_DUST_COMMITMENT_HASHES,
                );
                // n offers
                cost * outputs.len()
            }
            PayBlockRewardsToTreasury { .. } => {
                let mut cost = RunningCost::ZERO;
                cost += model.cell_read(16) + model.cell_write(16, true);
                cost += model.map_index(EXPECTED_TOKEN_TYPE_DEPTH);
                cost += model.map_insert(EXPECTED_TOKEN_TYPE_DEPTH, true);
                cost
            }
            PayFromTreasuryShielded { outputs, .. } => {
                let mut cost = RunningCost::ZERO;
                // Zswap UTXO creation
                // commitment hash
                cost += RunningCost::compute(model.runtime_cost_model.transient_hash);
                // merkle tree insertion
                cost += model.merkle_tree_insert_amortized(ZSWAP_TREE_HEIGHT as usize, false);
                //set insertion
                cost += model.map_insert(EXPECTED_UTXO_DEPTH, false)
                    + model.cell_write(PERSISTENT_HASH_BYTES as u64, false);
                cost = cost * outputs.len();
                // treasury subtraction
                cost += model.cell_read(16) + model.cell_write(16, true);
                cost += model.map_index(EXPECTED_TOKEN_TYPE_DEPTH);
                cost += model.map_insert(EXPECTED_TOKEN_TYPE_DEPTH, true);
                cost
            }
            PayFromTreasuryUnshielded { outputs, .. } => {
                let mut cost = RunningCost::ZERO;
                // Replay protection state update
                cost +=
                    model.time_filter_map_lookup() + model.cell_read(PERSISTENT_HASH_BYTES as u64);
                cost += model.time_filter_map_insert(false)
                    + model.cell_write(PERSISTENT_HASH_BYTES as u64, false);
                // Unshielded UTXO creation
                cost += model.map_insert(EXPECTED_UTXO_DEPTH, false);
                cost += model.cell_write(UTXO_SIZE as u64, false);
                cost = cost * outputs.len();
                // treasury subtraction
                cost += model.cell_read(16) + model.cell_write(16, true);
                cost += model.map_index(EXPECTED_TOKEN_TYPE_DEPTH);
                cost += model.map_insert(EXPECTED_TOKEN_TYPE_DEPTH, true);
                // Commitment merkle tree insertion & first-free update
                cost += model.cell_read(8) + model.cell_write(8, true);
                cost += model.merkle_tree_insert_amortized(EXPECTED_UTXO_DEPTH, false)
                    + model.cell_write(FR_BYTES as u64, false);
                // Commitment computation
                cost += RunningCost::compute(
                    model.runtime_cost_model.transient_hash * FRESH_DUST_COMMITMENT_HASHES,
                );
                cost
            }
            DistributeReserve(..) => {
                // changing two pool balances
                let cost = model.cell_read(16) + model.cell_write(16, true);
                cost * 2u64
            }
            CNightGeneratesDustUpdate { events } => {
                let creates = events
                    .iter()
                    .filter(|e| e.action == CNightGeneratesDustActionType::Create)
                    .count();
                let destroys = events.len() - creates;
                let mut ccost = RunningCost::ZERO;
                // Night generates Dust address table read
                ccost += model.map_index(EXPECTED_UTXO_DEPTH) + model.cell_read(FR_BYTES as u64);
                // Generation tree insertion & first-free update
                ccost += model.cell_read(8) + model.cell_write(8, true);
                ccost += model.merkle_tree_insert_amortized(32, false)
                    + model.cell_write(DUST_GENERATION_INFO_SIZE as u64, false);
                // Night indicies insertion
                ccost += model.map_insert(EXPECTED_UTXO_DEPTH, false) + model.cell_write(8, false);
                // Commitment merkle tree insertion & first-free update
                ccost += model.cell_read(8) + model.cell_write(8, true);
                ccost += model.merkle_tree_insert_amortized(EXPECTED_UTXO_DEPTH, false)
                    + model.cell_write(FR_BYTES as u64, false);
                // Commitment computation
                ccost += RunningCost::compute(
                    model.runtime_cost_model.transient_hash * FRESH_DUST_COMMITMENT_HASHES,
                );
                let mut dcost = RunningCost::ZERO;
                // Dtime update
                dcost += model.map_index(EXPECTED_GENERATION_DEPTH);
                dcost += model.merkle_tree_index(EXPECTED_GENERATION_DEPTH);
                dcost += model.merkle_tree_insert_unamortized(32, true);
                ccost * creates + dcost * destroys
            }
        }
        .into()
    }
}

#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    FieldRepr,
    FromFieldRepr,
    Serializable,
    Dummy,
    Storable,
)]
#[storable(base)]
#[tag = "transaction-hash"]
pub struct TransactionHash(pub HashOutput);
tag_enforcement_test!(TransactionHash);

impl Serialize for TransactionHash {
    fn serialize<S: serde::ser::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut bytes = Vec::new();
        <Self as Serializable>::serialize(self, &mut bytes).map_err(serde::ser::Error::custom)?;
        ser.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for TransactionHash {
    fn deserialize<D: serde::de::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        de.deserialize_bytes(BorshVisitor(PhantomData))
    }
}

#[cfg(feature = "proptest")]
impl rand::distributions::Distribution<ContractCall<(), InMemoryDB>>
    for rand::distributions::Standard
{
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> ContractCall<(), InMemoryDB> {
        ContractCall {
            address: rng.r#gen(),
            entry_point: rng.r#gen(),
            guaranteed_transcript: None, // rng.r#gen(), TODO WG
            fallible_transcript: None,   // rng.r#gen(), TODO WG
            communication_commitment: rng.r#gen(),
            proof: (),
        }
    }
}

#[derive(Storable)]
#[derive_where(Clone, PartialEq, Eq; P)]
#[storable(db = D)]
#[tag = "contract-call[v3]"]
pub struct ContractCall<P: ProofKind<D>, D: DB> {
    pub address: ContractAddress,
    pub entry_point: EntryPointBuf,
    // nb: Vector is *not* sorted
    pub guaranteed_transcript: Option<Sp<Transcript<D>, D>>,
    pub fallible_transcript: Option<Sp<Transcript<D>, D>>,

    pub communication_commitment: Fr,
    pub proof: P::Proof,
}
tag_enforcement_test!(ContractCall<(), InMemoryDB>);

impl<P: ProofKind<D>, D: DB> Debug for ContractCall<P, D> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter
            .debug_map()
            .entry(&Symbol("contract"), &self.address)
            .entry(&Symbol("entry_point"), &self.entry_point)
            .entry(
                &Symbol("guaranteed_transcript"),
                &self.guaranteed_transcript,
            )
            .entry(&Symbol("fallible_transcript"), &self.fallible_transcript)
            .entry(&Symbol("communication_commitment"), &Symbol("<commitment>"))
            .entry(&Symbol("proof"), &Symbol("<proof>"))
            .finish()
    }
}

impl<P: ProofKind<D>, D: DB> ContractCall<P, D> {
    pub fn context(
        self,
        block: &BlockContext,
        intent: &Intent<(), (), Pedersen, D>,
        state: ContractState<D>,
        com_indices: &Map<Commitment, u64>,
    ) -> CallContext<D> {
        let caller = intent
            .actions
            .iter_deref()
            .find_map(|action| match action {
                ContractAction::Call(caller) if caller.clone().calls(&self.erase_proof()) => {
                    Some(PublicAddress::Contract(caller.address))
                }
                _ => None,
            })
            .or_else(|| {
                let mut owners = intent
                    .guaranteed_unshielded_offer
                    .iter()
                    .chain(intent.fallible_unshielded_offer.iter())
                    .flat_map(|o| o.inputs.iter())
                    .map(|i| i.owner.clone());
                let owner = owners.next()?;
                if owners.all(|owner2| owner == owner2) {
                    Some(PublicAddress::User(UserAddress::from(owner)))
                } else {
                    None
                }
            });
        CallContext {
            own_address: self.address,
            tblock: block.tblock,
            tblock_err: block.tblock_err,
            parent_block_hash: block.parent_block_hash,
            caller,
            balance: state.balance,
            com_indices: com_indices.clone(),
            last_block_time: block.last_block_time,
        }
    }

    pub fn calls(&self, callee: &ContractCall<P, D>) -> bool {
        self.calls_with_seq(callee).is_some()
    }

    pub fn calls_with_seq(&self, callee: &ContractCall<P, D>) -> Option<(bool, u64)> {
        let calls: Vec<((u64, ContractAddress, HashOutput, Fr), bool)> = self
            .guaranteed_transcript
            .iter()
            .map(|x| (x, true))
            .chain(self.fallible_transcript.iter().map(|x| (x, false)))
            .flat_map(|(t, guaranteed)| {
                let ccs: HashSet<ClaimedContractCallsValue, D> =
                    t.deref().effects.claimed_contract_calls.clone();
                ccs.iter()
                    .map(|x| ((*x).deref().into_inner(), guaranteed))
                    .collect::<Vec<_>>()
                    .clone()
            })
            .collect();
        calls
            .into_iter()
            .find_map(|((seq, addr, ep, cc), guaranteed)| {
                (addr == callee.address
                    && ep == callee.entry_point.ep_hash()
                    && cc == callee.communication_commitment)
                    .then_some((guaranteed, seq))
            })
    }

    pub fn erase_proof(&self) -> ContractCall<(), D> {
        ContractCall {
            address: self.address,
            entry_point: self.entry_point.clone(),
            guaranteed_transcript: self.guaranteed_transcript.clone(),
            fallible_transcript: self.fallible_transcript.clone(),
            communication_commitment: self.communication_commitment,
            proof: (),
        }
    }
}

#[derive(Storable)]
#[derive_where(Clone, PartialEq, Eq)]
#[storable(db = D)]
#[tag = "contract-deploy[v4]"]
pub struct ContractDeploy<D: DB> {
    pub initial_state: ContractState<D>,
    pub nonce: HashOutput,
}
tag_enforcement_test!(ContractDeploy<InMemoryDB>);

impl<D: DB> Debug for ContractDeploy<D> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("Deploy ")?;
        self.initial_state.fmt(formatter)
    }
}

impl<D: DB> ContractDeploy<D> {
    pub fn address(&self) -> ContractAddress {
        let mut writer = Sha256::new();
        tagged_serialize(self, &mut writer).expect("In-memory serialization should succeed");
        ContractAddress(HashOutput(writer.finalize().into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ContractOperationVersion {
    V3,
}

impl Serializable for ContractOperationVersion {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), std::io::Error> {
        use ContractOperationVersion as V;
        match self {
            V::V3 => Serializable::serialize(&2u8, writer),
        }
    }

    fn serialized_size(&self) -> usize {
        use ContractOperationVersion as V;
        match self {
            V::V3 => 1,
        }
    }
}

impl Tagged for ContractOperationVersion {
    fn tag() -> std::borrow::Cow<'static, str> {
        "contract-operation-version".into()
    }
    fn tag_unique_factor() -> String {
        "u8".into()
    }
}
tag_enforcement_test!(ContractOperationVersion);

impl Deserializable for ContractOperationVersion {
    fn deserialize(reader: &mut impl Read, _recursion_depth: u32) -> Result<Self, std::io::Error> {
        use ContractOperationVersion as V;
        let mut disc = vec![0u8; 1];
        reader.read_exact(&mut disc)?;
        match disc[0] {
            0u8..=1u8 => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid old discriminant {}", disc[0]),
            )),
            2u8 => Ok(V::V3),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unknown discriminant {}", disc[0]),
            )),
        }
    }
}

impl ContractOperationVersion {
    pub(crate) fn has(&self, co: &ContractOperation) -> bool {
        use ContractOperationVersion as V;
        match self {
            V::V3 => co.v2.is_some(),
        }
    }
    pub(crate) fn rm_from(&self, co: &mut ContractOperation) {
        use ContractOperationVersion as V;
        match self {
            V::V3 => co.v2 = None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ContractOperationVersionedVerifierKey {
    V3(transient_crypto::proofs::VerifierKey),
}

impl ContractOperationVersionedVerifierKey {
    pub(crate) fn as_version(&self) -> ContractOperationVersion {
        use ContractOperationVersion as V;
        use ContractOperationVersionedVerifierKey as VK;
        match self {
            VK::V3(_) => V::V3,
        }
    }

    pub(crate) fn insert_into(&self, co: &mut ContractOperation) {
        use ContractOperationVersionedVerifierKey as VK;
        match self {
            VK::V3(vk) => co.v2 = Some(vk.clone()),
        }
    }
}

impl Serializable for ContractOperationVersionedVerifierKey {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), std::io::Error> {
        use ContractOperationVersionedVerifierKey as VK;
        match self {
            VK::V3(vk) => {
                Serializable::serialize(&2u8, writer)?;
                Serializable::serialize(vk, writer)
            }
        }
    }

    fn serialized_size(&self) -> usize {
        use ContractOperationVersionedVerifierKey as VK;
        match self {
            VK::V3(vk) => 1 + Serializable::serialized_size(vk),
        }
    }
}

impl Tagged for ContractOperationVersionedVerifierKey {
    fn tag() -> std::borrow::Cow<'static, str> {
        "contract-operation-versioned-verifier-key".into()
    }
    fn tag_unique_factor() -> String {
        format!("[[],[],{}]", transient_crypto::proofs::VerifierKey::tag())
    }
}
tag_enforcement_test!(ContractOperationVersionedVerifierKey);

impl Deserializable for ContractOperationVersionedVerifierKey {
    fn deserialize(reader: &mut impl Read, recursion_depth: u32) -> Result<Self, std::io::Error> {
        use ContractOperationVersionedVerifierKey as VK;
        let mut disc = vec![0u8; 1];
        reader.read_exact(&mut disc)?;
        match disc[0] {
            0u8..=1u8 => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid old discriminant {}", disc[0]),
            )),
            2u8 => Ok(VK::V3(Deserializable::deserialize(
                reader,
                recursion_depth,
            )?)),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unknown discriminant {}", disc[0]),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serializable, Storable)]
#[storable(base)]
#[tag = "maintenance-update-single-update[v1]"]
pub enum SingleUpdate {
    /// Replaces the authority for this contract.
    /// Any subsequent updates in this update sequence are still carried out.
    ReplaceAuthority(ContractMaintenanceAuthority),
    /// Removes a verifier key associated with a given version and entry point.
    VerifierKeyRemove(EntryPointBuf, ContractOperationVersion),
    /// Inserts a new verifier key under a given version and entry point.
    /// This operations *does not* replace existing keys, which must first be
    /// explicitly removed.
    VerifierKeyInsert(EntryPointBuf, ContractOperationVersionedVerifierKey),
}
tag_enforcement_test!(SingleUpdate);

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serializable, Storable, Debug)]
#[storable(base)]
#[tag = "maintenance-update-signatures-value[v1]"]
// This type exists solely to work nicely with storage. It's the tuple of `(index, signature)` for the elements of `MaintenanceUpdate::signatures`
pub struct SignaturesValue(pub u32, pub Signature);
tag_enforcement_test!(SignaturesValue);

impl SignaturesValue {
    pub fn into_inner(&self) -> (u32, Signature) {
        (self.0, self.1.clone())
    }
}

#[derive(Storable)]
#[derive_where(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[tag = "contract-maintenance-update[v1]"]
#[storable(db = D)]
pub struct MaintenanceUpdate<D: DB> {
    pub address: ContractAddress,
    pub updates: storage::storage::Array<SingleUpdate, D>,
    pub counter: u32,
    pub signatures: storage::storage::Array<SignaturesValue, D>,
}
tag_enforcement_test!(MaintenanceUpdate<InMemoryDB>);

impl<D: DB> Debug for MaintenanceUpdate<D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MaintenanceUpdate")
            .field("address", &self.address)
            .field("updates", &self.updates)
            .field("counter", &self.counter)
            .field("signatures", &self.signatures)
            .finish()
    }
}

impl<D: DB> MaintenanceUpdate<D> {
    pub fn data_to_sign(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend(b"midnight:contract-update:");
        Serializable::serialize(&self.address, &mut data)
            .expect("In-memory serialization should succeed");
        Serializable::serialize(&self.updates, &mut data)
            .expect("In-memory serialization should succeed");
        Serializable::serialize(&self.counter, &mut data)
            .expect("In-memory serialization should succeed");
        data
    }
}

#[derive(Storable)]
#[storable(db = D)]
#[tag = "contract-action[v6]"]
#[derive_where(Clone, PartialEq, Eq; P)]
pub enum ContractAction<P: ProofKind<D>, D: DB> {
    Call(#[storable(child)] Sp<ContractCall<P, D>, D>),
    Deploy(Sp<ContractDeploy<D>, D>),
    Maintain(MaintenanceUpdate<D>),
}
tag_enforcement_test!(ContractAction<(), InMemoryDB>);

impl<P: ProofKind<D>, D: DB> From<ContractCall<P, D>> for ContractAction<P, D> {
    fn from(call: ContractCall<P, D>) -> Self {
        ContractAction::Call(Sp::new(call))
    }
}

impl<P: ProofKind<D>, D: DB> From<ContractDeploy<D>> for ContractAction<P, D> {
    fn from(deploy: ContractDeploy<D>) -> Self {
        ContractAction::Deploy(Sp::new(deploy))
    }
}

impl<P: ProofKind<D>, D: DB> From<MaintenanceUpdate<D>> for ContractAction<P, D> {
    fn from(upd: MaintenanceUpdate<D>) -> Self {
        ContractAction::Maintain(upd)
    }
}

impl<P: ProofKind<D>, D: DB> Debug for ContractAction<P, D> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match self {
            ContractAction::Call(call) => call.fmt(formatter),
            ContractAction::Deploy(deploy) => deploy.fmt(formatter),
            ContractAction::Maintain(upd) => upd.fmt(formatter),
        }
    }
}

impl<P: ProofKind<D>, D: DB> ContractAction<P, D> {
    pub fn erase_proof(&self) -> ContractAction<(), D> {
        match self {
            ContractAction::Call(call) => ContractAction::Call(Sp::new(call.erase_proof())),
            ContractAction::Deploy(deploy) => ContractAction::Deploy(deploy.clone()),
            ContractAction::Maintain(upd) => ContractAction::Maintain(upd.clone()),
        }
    }

    pub fn erase_proofs(actions: Vec<&ContractAction<P, D>>) -> Vec<ContractAction<(), D>> {
        actions
            .into_iter()
            .map(ContractAction::erase_proof)
            .collect()
    }

    pub fn challenge_pre_for(calls: &[ContractAction<P, D>]) -> Vec<u8> {
        let mut data = Vec::new();
        for cd in calls.iter() {
            match cd {
                ContractAction::Call(call) => {
                    data.push(0u8);
                    let _ = Serializable::serialize(&call.address, &mut data);
                    let _ = Serializable::serialize(&call.entry_point, &mut data);

                    let _ = Serializable::serialize(&call.guaranteed_transcript, &mut data);

                    let _ = Serializable::serialize(&call.fallible_transcript, &mut data);
                }
                ContractAction::Deploy(deploy) => {
                    data.push(1u8);
                    let _ = Serializable::serialize(&deploy, &mut data);
                }
                ContractAction::Maintain(upd) => {
                    data.push(2u8);
                    let _ = Serializable::serialize(&upd, &mut data);
                }
            }
        }
        data
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serializable)]
#[tag = "transcation-id[v1]"]
pub enum TransactionIdentifier {
    Merged(Pedersen),
    Unique(HashOutput),
}
tag_enforcement_test!(TransactionIdentifier);

impl Serialize for TransactionIdentifier {
    fn serialize<S: serde::ser::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut bytes = Vec::new();
        <Self as Serializable>::serialize(self, &mut bytes).map_err(serde::ser::Error::custom)?;
        ser.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for TransactionIdentifier {
    fn deserialize<D: serde::de::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        de.deserialize_bytes(BorshVisitor(PhantomData))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serializable, Storable)]
#[tag = "unshielded-utxo[v1]"]
#[storable(base)]
pub struct Utxo {
    pub value: u128,
    pub owner: UserAddress,
    pub type_: UnshieldedTokenType,
    pub intent_hash: IntentHash,
    pub output_no: u32,
}
tag_enforcement_test!(Utxo);

pub(crate) const UTXO_SIZE: usize = 16 + PERSISTENT_HASH_BYTES * 3 + 4;

impl From<UtxoSpend> for Utxo {
    fn from(x: UtxoSpend) -> Self {
        Utxo {
            value: x.value,
            owner: UserAddress::from(x.owner),
            type_: x.type_,
            intent_hash: x.intent_hash,
            output_no: x.output_no,
        }
    }
}

#[derive(Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Serializable, Storable)]
#[storable(base)]
#[tag = "unshielded-utxo-output[v1]"]
pub struct UtxoOutput {
    pub value: u128,
    pub owner: UserAddress,
    pub type_: UnshieldedTokenType,
}
tag_enforcement_test!(UtxoOutput);

impl From<Utxo> for UtxoOutput {
    fn from(value: Utxo) -> Self {
        UtxoOutput {
            value: value.value,
            owner: value.owner,
            type_: value.type_,
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serializable, Storable)]
#[storable(base)]
#[tag = "unshielded-utxo-spend"]
pub struct UtxoSpend {
    pub value: u128,
    pub owner: VerifyingKey,
    pub type_: UnshieldedTokenType,
    pub intent_hash: IntentHash,
    pub output_no: u32,
}
tag_enforcement_test!(UtxoSpend);

#[derive(PartialOrd, Ord, Debug, Clone, PartialEq, Eq, Hash, Serializable, Storable)]
#[storable(base)]
#[tag = "utxo-metadata[v1]"]
pub struct UtxoMeta {
    pub ctime: Timestamp,
}
tag_enforcement_test!(UtxoMeta);

#[derive(Storable, PartialOrd, Ord)]
#[derive_where(Debug, Clone, PartialEq, Eq, Hash)]
#[storable(db = D)]
#[tag = "unshielded-utxo-state[v2]"]
#[must_use]
pub struct UtxoState<D: DB> {
    pub utxos: HashMap<Utxo, UtxoMeta, D, NightAnn>,
}
tag_enforcement_test!(UtxoState<InMemoryDB>);

impl Annotation<Utxo> for NightAnn {
    fn from_value(utxo: &Utxo) -> Self {
        NightAnn {
            size: 1,
            value: if utxo.type_ == NIGHT { utxo.value } else { 0 },
        }
    }
}

impl<D: DB> Annotation<(Sp<Utxo, D>, Sp<UtxoMeta, D>)> for NightAnn {
    fn from_value(tuple: &(Sp<Utxo, D>, Sp<UtxoMeta, D>)) -> Self {
        Self::from_value(tuple.0.deref())
    }
}

impl<D: DB> Annotation<(Sp<Utxo, D>, Sp<(), D>)> for NightAnn {
    fn from_value(tuple: &(Sp<Utxo, D>, Sp<(), D>)) -> Self {
        Self::from_value(tuple.0.deref())
    }
}

impl Annotation<u128> for NightAnn {
    fn from_value(value: &u128) -> Self {
        NightAnn {
            size: 1,
            value: *value,
        }
    }
}

impl<D: DB> Annotation<ContractState<D>> for NightAnn {
    fn from_value(state: &ContractState<D>) -> Self {
        NightAnn {
            size: 1,
            value: state
                .balance
                .get(&TokenType::Unshielded(NIGHT))
                .map(|sp_value| *sp_value)
                .unwrap_or(0),
        }
    }
}

impl<D: DB> UtxoState<D> {
    pub fn insert(&self, utxo: Utxo, meta: UtxoMeta) -> Self {
        UtxoState {
            utxos: self.utxos.insert(utxo, meta),
        }
    }

    pub fn remove(&self, utxo: &Utxo) -> Self {
        UtxoState {
            utxos: self.utxos.remove(utxo),
        }
    }
}

impl<D: DB> Default for UtxoState<D> {
    fn default() -> Self {
        Self {
            utxos: HashMap::new(),
        }
    }
}

#[derive(Storable)]
#[derive_where(Clone, Debug, PartialEq, Eq)]
#[storable(db = D)]
#[tag = "ledger-state[v13]"]
#[must_use]
pub struct LedgerState<D: DB> {
    pub network_id: String,
    #[storable(child)]
    pub parameters: Sp<LedgerParameters, D>,
    pub locked_pool: u128,
    pub bridge_receiving: Map<UserAddress, u128, D>,
    pub reserve_pool: u128,
    pub block_reward_pool: u128,
    pub unclaimed_block_rewards: Map<UserAddress, u128, D, NightAnn>,
    pub treasury: Map<TokenType, u128, D>,
    #[storable(child)]
    pub zswap: Sp<zswap::ledger::State<D>, D>,
    pub contract: Map<ContractAddress, ContractState<D>, D, NightAnn>,
    #[storable(child)]
    pub utxo: Sp<UtxoState<D>, D>,
    pub replay_protection: Sp<ReplayProtectionState<D>, D>,
    #[storable(child)]
    pub dust: Sp<DustState<D>, D>,
}
tag_enforcement_test!(LedgerState<InMemoryDB>);

/// The maximum rewardable supply of NIGHT atomic units. 24 billion NIGHT
/// with an atomic unit at 10^-6.
#[allow(clippy::inconsistent_digit_grouping)]
pub const MAX_SUPPLY: u128 = 24_000_000_000 * STARS_PER_NIGHT;
pub const STARS_PER_NIGHT: u128 = 1_000_000;
pub const SPECKS_PER_DUST: u128 = 1_000_000_000_000_000;

impl<D: DB> LedgerState<D> {
    /// Constructs a new ledger state with default parameters.
    pub fn new(network_id: impl Into<String>) -> Self {
        Self::with_genesis_settings(network_id, INITIAL_PARAMETERS, 0, MAX_SUPPLY, 0)
            .expect("default state should be legal")
    }

    /// Constructs a new ledger state with given genesis parameterisation.
    ///
    /// From the ledger's perspective, this includes the network ID, initial parameters, as well as
    /// three system-controlled pools:
    /// - The locked pool, which represents funds locked on Midnight due to being circulating as
    ///   cNIGHT on Cardano
    /// - The reserve pool, which represents funds set aside as a reward for rewarding protocol
    ///   participation
    /// - The treasury pool, which holds funds to be controlled by governance for strategic use (at
    ///   genesis, this should equal the Illiquid Circulating Supply, or ICS on Cardano, which
    ///   represents tokens locked on Cardano, which at genesis is *only* the treasury.
    pub fn with_genesis_settings(
        network_id: impl Into<String>,
        parameters: LedgerParameters,
        locked_pool: u128,
        reserve_pool: u128,
        treasury: u128,
    ) -> Result<Self, InvariantViolation> {
        let result = LedgerState {
            network_id: network_id.into(),
            parameters: Sp::new(parameters),
            locked_pool,
            bridge_receiving: Map::new(),
            reserve_pool,
            treasury: [(TokenType::Unshielded(NIGHT), treasury)]
                .into_iter()
                .collect(),
            block_reward_pool: 0,
            unclaimed_block_rewards: Map::new(),
            zswap: Sp::new(zswap::ledger::State::new()),
            contract: Map::new(),
            utxo: Sp::new(UtxoState::default()),
            replay_protection: Sp::new(ReplayProtectionState::default()),
            dust: Sp::new(DustState::default()),
        };
        result.check_night_balance_invariant()?;
        Ok(result)
    }

    pub fn state_hash(&self) -> ArenaHash<D::Hasher> {
        Sp::new(self.clone()).hash()
    }

    pub fn index(&self, address: ContractAddress) -> Option<ContractState<D>> {
        self.contract.get(&address).cloned()
    }

    pub fn update_index(
        &self,
        address: ContractAddress,
        state: ChargedState<D>,
        balance: storage::storage::HashMap<TokenType, u128, D>,
    ) -> Self {
        let mut new_ledger_state = self.clone();
        let contract = new_ledger_state
            .contract
            .get(&address)
            .cloned()
            .unwrap_or_default();
        let new_contract = ContractState {
            data: state,
            balance,
            ..contract
        };
        new_ledger_state.contract = new_ledger_state.contract.insert(address, new_contract);
        new_ledger_state
    }
}

pub(crate) struct Symbol(pub(crate) &'static str);

impl Debug for Symbol {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

struct BorshVisitor<T>(PhantomData<T>);

impl<T: Deserializable> serde::de::Visitor<'_> for BorshVisitor<T> {
    type Value = T;
    fn expecting(&self, fmt: &mut Formatter) -> fmt::Result {
        write!(fmt, "Borsh-serialized {}", std::any::type_name::<T>())
    }
    fn visit_bytes<E: serde::de::Error>(self, mut v: &[u8]) -> Result<Self::Value, E> {
        Deserializable::deserialize(&mut v, 0).map_err(serde::de::Error::custom)
    }
}

pub const FEE_TOKEN: TokenType = TokenType::Dust;

#[cfg(test)]
mod tests {
    use storage::db::InMemoryDB;

    use super::*;

    #[test]
    fn test_max_price_adjustment() {
        let adj = f64::from(INITIAL_PARAMETERS.max_price_adjustment());
        assert!(1.045 <= adj);
        assert!(adj <= 1.047);
    }

    #[test]
    fn test_genesis_state() {
        assert_eq!(
            LedgerState::<InMemoryDB>::with_genesis_settings(
                "local-test",
                INITIAL_PARAMETERS,
                10_000_000_000_000_000,
                10_000_000_000_000_000,
                10_000_000_000_000_000,
            ),
            Err(InvariantViolation::NightBalance(30_000_000_000_000_000))
        );
        assert_eq!(
            LedgerState::<InMemoryDB>::with_genesis_settings(
                "local-test",
                INITIAL_PARAMETERS,
                1_000_000_000_000_000,
                1_000_000_000_000_000,
                1_000_000_000_000_000,
            ),
            Err(InvariantViolation::NightBalance(3_000_000_000_000_000))
        );
        assert!(
            LedgerState::<InMemoryDB>::with_genesis_settings(
                "local-test",
                INITIAL_PARAMETERS,
                10_000_000_000_000_000,
                10_000_000_000_000_000,
                4_000_000_000_000_000,
            )
            .is_ok()
        );
    }

    #[test]
    fn test_state_serialized() {
        let state = LedgerState::<InMemoryDB>::new("local-test");
        let mut ser = Vec::new();
        serialize::tagged_serialize(&state, &mut ser).unwrap();
        let _ = serialize::tagged_deserialize::<LedgerState<InMemoryDB>>(&ser[..]).unwrap();
    }
}
