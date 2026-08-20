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

use core::fmt::Debug;
use core::fmt::Formatter;
use std::borrow::Cow;

use base_crypto::hash::{PERSISTENT_HASH_BYTES, PersistentHashWriter};
use base_crypto::repr::MemWrite;
use coin_structure::coin::{
    self, Commitment, Info as CoinInfo, Nullifier, QualifiedInfo as QualifiedCoinInfo,
};
use coin_structure::transfer::{Recipient, SenderEvidence};
use derive_where::derive_where;
use rand::{CryptoRng, Rng};
use serialize::tag_enforcement_test;
use serialize::{Deserializable, Serializable, Tagged};
use storage::Storable;
use storage::arena::{ArenaHash, ArenaKey};
use storage::db::DB;
use storage::storable::Loader;
use storage::storage::default_storage;
use storage::storage::{HashMap, Map};
use transient_crypto::encryption;
use transient_crypto::merkle_tree::{self, InvalidUpdate, MerkleTree, MerkleTreeCollapsedUpdate};
use transient_crypto::proofs::ProofPreimage;
use transient_crypto::repr::FieldRepr;

use crate::ZSWAP_TREE_HEIGHT;
use crate::error::OfferCreationFailed;
use crate::keys::{SecretKeys, Seed};
use crate::structure::*;

#[derive(Debug, Storable)]
#[derive_where(Clone)]
#[storable(db = D)]
#[tag = "zswap-local-state[v6]"]
#[must_use]
pub struct State<D: DB> {
    pub coins: Map<Nullifier, QualifiedCoinInfo, D>,
    pub pending_spends: Map<Nullifier, QualifiedCoinInfo, D>,
    pub pending_outputs: Map<Commitment, CoinInfo, D>,
    pub merkle_tree: MerkleTree<(), D>,
    pub first_free: u64,
}
tag_enforcement_test!(State<storage::db::InMemoryDB>);

impl<D: DB> Default for State<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: DB> State<D> {
    pub fn new() -> Self {
        State {
            coins: Map::new(),
            pending_spends: Map::new(),
            pending_outputs: Map::new(),
            merkle_tree: MerkleTree::blank(ZSWAP_TREE_HEIGHT),
            first_free: 0,
        }
    }

    pub fn apply_collapsed_update(
        &self,
        update: &MerkleTreeCollapsedUpdate,
    ) -> Result<Self, InvalidUpdate> {
        Ok(Self {
            merkle_tree: self.merkle_tree.apply_collapsed_update(update)?.rehash(),
            first_free: u64::max(self.first_free, update.end + 1),
            ..self.clone()
        })
    }

    pub fn insert_coin(
        &self,
        secret_keys: &SecretKeys,
        coin: CoinInfo,
    ) -> Result<Self, InvalidUpdate> {
        let qcoin = coin.qualify(self.first_free);
        let nullifier = coin.nullifier(&SenderEvidence::User(Cow::Borrowed(
            &secret_keys.coin_secret_key,
        )));
        Ok(Self {
            coins: self.coins.insert(nullifier, qcoin),
            merkle_tree: self
                .merkle_tree
                .try_update_hash(
                    self.first_free,
                    coin.commitment(&Recipient::User(secret_keys.coin_public_key()))
                        .0,
                    (),
                )?
                .rehash(),
            first_free: self.first_free + 1,
            ..self.clone()
        })
    }

    pub fn remove_coin_by_nullifier(&self, nullifier: Nullifier) -> Self {
        Self {
            coins: self.coins.remove(&nullifier),
            pending_spends: self.pending_spends.remove(&nullifier),
            ..self.clone()
        }
    }

    pub fn apply_claim<P: Storable<D>>(
        &self,
        secret_keys: &SecretKeys,
        tx: &AuthorizedClaim<P>,
    ) -> Result<Self, InvalidUpdate> {
        let mut res = self.clone();
        res.merkle_tree = res
            .merkle_tree
            .try_update_hash(
                res.first_free,
                tx.coin.commitment(&Recipient::User(tx.recipient)).0,
                (),
            )?
            .rehash();
        if secret_keys.coin_public_key() == tx.recipient {
            res.coins = self.coins.insert(
                tx.coin.nullifier(&SenderEvidence::User(Cow::Borrowed(
                    &secret_keys.coin_secret_key,
                ))),
                tx.coin.qualify(self.first_free),
            );
        } else {
            res.merkle_tree.collapse(res.first_free, res.first_free);
        }
        res.first_free += 1;
        Ok(res)
    }

    #[instrument(skip(self, tx))]
    pub fn apply_failed<P: Storable<D>>(&self, tx: &Offer<P, D>) -> State<D> {
        let mut res = self.clone();
        for nullifier in tx
            .inputs
            .iter_deref()
            .map(|o| &o.nullifier)
            .chain(tx.transient.iter_deref().map(|io| &io.nullifier))
        {
            res.pending_spends = res.pending_spends.remove(nullifier);
        }
        for coin_com in tx
            .outputs
            .iter_deref()
            .map(|o| &o.coin_com)
            .chain(tx.transient.iter_deref().map(|io| &io.coin_com))
        {
            res.pending_outputs = res.pending_outputs.remove(coin_com);
        }
        res
    }

    #[instrument(skip(self, tx))]
    pub fn apply<P: Storable<D>>(
        &self,
        secret_keys: &SecretKeys,
        tx: &Offer<P, D>,
    ) -> Result<State<D>, InvalidUpdate> {
        let mut res = self.clone();
        for (coin_com, ciph) in tx
            .outputs
            .iter_deref()
            .map(|o| (&o.coin_com, &o.ciphertext))
            .chain(
                tx.transient
                    .iter_deref()
                    .map(|io| (&io.coin_com, &io.ciphertext)),
            )
        {
            res.merkle_tree = res
                .merkle_tree
                .try_update_hash(res.first_free, coin_com.0, ())?;
            if let Some(ci) = ciph.as_ref().and_then(|ciph| secret_keys.try_decrypt(ciph)) {
                info!(coin=?ci, "received coin");
                let qci = ci.qualify(res.first_free);
                // Verify that what we got is actually valid.
                if &ci.commitment(&Recipient::User(secret_keys.coin_public_key())) == coin_com {
                    res.coins = res.coins.insert(
                        CoinInfo::nullifier(
                            &(&qci).into(),
                            &SenderEvidence::User(Cow::Borrowed(&secret_keys.coin_secret_key)),
                        ),
                        qci,
                    );
                    res.pending_outputs = res.pending_outputs.remove(coin_com);
                }
            } else if let Some(coin) = res.pending_outputs.get(coin_com) {
                info!(?coin, "received coin");
                let qci = coin.qualify(res.first_free);
                res.coins = res.coins.insert(
                    CoinInfo::nullifier(
                        &(&qci).into(),
                        &SenderEvidence::User(Cow::Borrowed(&secret_keys.coin_secret_key)),
                    ),
                    qci,
                );
                res.pending_outputs = res.pending_outputs.remove(coin_com);
            } else {
                res.merkle_tree = res.merkle_tree.collapse(res.first_free, res.first_free);
            }
            res.first_free += 1;
        }
        for nul in tx
            .inputs
            .iter_deref()
            .map(|i| &i.nullifier)
            .chain(tx.transient.iter_deref().map(|io| &io.nullifier))
        {
            if let Some(coin) = res.coins.get(nul) {
                info!(?coin, "spent coin finalized");
                res.coins = res.coins.remove(nul);
            }
            if let Some(coin) = res.pending_spends.get(nul) {
                info!(?coin, "pending spend removed");
                res.pending_spends = res.pending_spends.remove(nul);
            }
        }
        res.merkle_tree = res.merkle_tree.rehash();
        Ok(res)
    }

    #[instrument(skip(self, rng))]
    pub fn spend<R: Rng + CryptoRng + ?Sized>(
        &self,
        rng: &mut R,
        secret_keys: &SecretKeys,
        coin: &QualifiedCoinInfo,
        segment: Option<u16>,
    ) -> Result<(State<D>, Input<ProofPreimage, D>), OfferCreationFailed> {
        self.spend_from_tree(rng, secret_keys, coin, segment, &self.merkle_tree.clone())
    }

    #[instrument(skip(self, rng, tree))]
    fn spend_from_tree<R: Rng + CryptoRng + ?Sized>(
        &self,
        rng: &mut R,
        secret_keys: &SecretKeys,
        coin: &QualifiedCoinInfo,
        segment: Option<u16>,
        tree: &MerkleTree<(), D>,
    ) -> Result<(State<D>, Input<ProofPreimage, D>), OfferCreationFailed> {
        let inp = Input::new_from_secret_key(
            rng,
            coin,
            segment,
            SenderEvidence::User(Cow::Borrowed(&secret_keys.coin_secret_key)),
            tree,
        )?;
        let res = State {
            pending_spends: self.pending_spends.insert(inp.nullifier, *coin),
            ..self.clone()
        };
        Ok((res, inp))
    }

    #[instrument(skip(self, rng))]
    pub fn spend_from_output<R: Rng + CryptoRng + ?Sized>(
        &self,
        rng: &mut R,
        secret_keys: &SecretKeys,
        coin: &QualifiedCoinInfo,
        segment: Option<u16>,
        output: Output<ProofPreimage, D>,
    ) -> Result<(State<D>, Transient<ProofPreimage, D>), OfferCreationFailed> {
        let tree = MerkleTree::blank(ZSWAP_TREE_HEIGHT)
            .try_update_hash(0, output.coin_com.0, ())
            .map_err(OfferCreationFailed::MerkleTreeError)?
            .rehash();
        let (res, input) = self.spend_from_tree(rng, secret_keys, coin, segment, &tree)?;
        let io = Transient {
            nullifier: input.nullifier,
            coin_com: output.coin_com,
            value_commitment_input: input.value_commitment,
            value_commitment_output: output.value_commitment,
            contract_address: output.contract_address,
            ciphertext: output.ciphertext,
            proof_input: input.proof,
            proof_output: output.proof,
        };
        Ok((res, io))
    }

    #[instrument(skip(rng))]
    pub fn authorize_claim<R: Rng + CryptoRng + ?Sized>(
        &self,
        rng: &mut R,
        secret_keys: &SecretKeys,
        coin: CoinInfo,
    ) -> Result<AuthorizedClaim<ProofPreimage>, OfferCreationFailed> {
        AuthorizedClaim::new::<R, D>(rng, coin, &secret_keys.coin_secret_key)
    }

    pub fn watch_for(&self, coin_public_key: &coin::PublicKey, coin: &CoinInfo) -> State<D> {
        debug!(?coin, "watching for coin");
        State {
            pending_outputs: self
                .pending_outputs
                .insert(coin.commitment(&Recipient::User(*coin_public_key)), *coin),
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use coin::ShieldedTokenType;
    use hex::FromHex;
    use rand::rngs::OsRng;
    use serde::{Deserialize, Deserializer};

    use super::*;

    #[test]
    fn coin_encryption_succeeds() {
        // Just tries to encrypt a dummy coin. This mainly tests that
        // COIN_CIPHERTEXT_LEN is accurate.
        let keys: SecretKeys = Seed::random(&mut OsRng).into();
        let coin = CoinInfo {
            nonce: OsRng.r#gen(),
            type_: ShieldedTokenType(OsRng.r#gen()),
            value: OsRng.r#gen(),
        };
        CoinCiphertext::new(&mut OsRng, &coin, keys.enc_public_key());
    }
}
