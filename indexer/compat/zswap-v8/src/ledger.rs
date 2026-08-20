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

use crate::ZSWAP_TREE_HEIGHT;
use crate::error::TransactionInvalid;
use crate::structure::*;
use base_crypto::time::{Duration, Timestamp};
use coin_structure::coin::{Commitment, Nullifier};
use coin_structure::contract::ContractAddress;
use derive_where::derive_where;
use serde::Serialize;
use serialize::{Deserializable, Serializable, Tagged, tag_enforcement_test};
use std::fmt::Debug;
use std::ops::Deref;
use storage::arena::Sp;
use storage::db::DB;
use storage::storage::default_storage;
use storage::storage::{HashMap, Map};
use storage::storage::{Identity, TimeFilterMap};
use storage::{
    Storable,
    arena::{ArenaHash, ArenaKey},
    storable::Loader,
};
use transient_crypto::merkle_tree::{MerkleTree, MerkleTreeDigest};

#[derive(Storable)]
#[derive_where(Clone, PartialEq, Debug, Eq)]
#[storable(db = D)]
#[tag = "zswap-ledger-state[v5]"]
#[must_use]
pub struct State<D: DB> {
    pub coin_coms: MerkleTree<Option<Sp<ContractAddress, D>>, D>,
    pub coin_coms_set: HashMap<Commitment, (), D>,
    pub first_free: u64,
    pub nullifiers: HashMap<Nullifier, (), D>,
    pub past_roots: TimeFilterMap<Identity<MerkleTreeDigest>, D>,
}
tag_enforcement_test!(State<storage::db::InMemoryDB>);

impl<D: DB> Default for State<D> {
    fn default() -> Self {
        State {
            coin_coms: MerkleTree::blank(ZSWAP_TREE_HEIGHT),
            coin_coms_set: HashMap::new(),
            first_free: 0,
            nullifiers: HashMap::new(),
            past_roots: TimeFilterMap::new(),
        }
    }
}

impl<D: DB> State<D> {
    pub fn new() -> Self {
        Default::default()
    }

    fn apply_input<P: Storable<D>>(
        mut self,
        inp: Input<P, D>,
        whitelist: &Option<Map<ContractAddress, ()>>,
    ) -> Result<Self, TransactionInvalid> {
        if !self.past_roots.contains(&inp.merkle_tree_root) {
            warn!(
                ?inp.merkle_tree_root,
                "attempted spend with unknown Merkle tree"
            );
            return Err(TransactionInvalid::UnknownMerkleRoot(inp.merkle_tree_root));
        };

        if self.nullifiers.contains_key(&inp.nullifier) {
            warn!(?inp.nullifier, "attempted double spend");
            return Err(TransactionInvalid::NullifierAlreadyPresent(inp.nullifier));
        }

        if Self::on_whitelist(
            whitelist,
            &(inp.contract_address.as_ref().map(|x| *x.deref())),
        ) {
            self.nullifiers = self.nullifiers.insert(inp.nullifier, ());
        }
        Ok(self)
    }

    fn apply_output<P: Storable<D>>(
        mut self,
        out: Output<P, D>,
        whitelist: &Option<Map<ContractAddress, ()>>,
    ) -> Result<(Self, Commitment, u64), TransactionInvalid> {
        if self.coin_coms_set.contains_key(&out.coin_com) {
            warn!(?out.coin_com, "attempted faerie gold");
            return Err(TransactionInvalid::CommitmentAlreadyPresent(out.coin_com));
        }
        self.coin_coms_set = self.coin_coms_set.insert(out.coin_com, ());
        let first_free = self.first_free;
        self.coin_coms = self
            .coin_coms
            .try_update_hash(
                first_free,
                out.coin_com.0,
                out.contract_address.as_ref().map(|x| Sp::new(*x.deref())),
            )
            .map_err(TransactionInvalid::MerkleTreeError)?;

        if !Self::on_whitelist(
            whitelist,
            &out.contract_address.as_ref().map(|x| *x.deref()),
        ) {
            self.coin_coms = self.coin_coms.collapse(first_free, first_free);
        }

        self.first_free = first_free + 1;
        Ok((self, out.coin_com, first_free)) // Different from the spec because I'm referring to the pre-plus-1 value
    }

    fn apply_transient<P: Storable<D>>(
        mut self,
        trans: Transient<P, D>,
        whitelist: &Option<Map<ContractAddress, ()>>,
    ) -> Result<(Self, Commitment, u64), TransactionInvalid> {
        if self.coin_coms_set.contains_key(&trans.coin_com) {
            warn!(?trans.coin_com, "attempted faerie gold");
            return Err(TransactionInvalid::CommitmentAlreadyPresent(trans.coin_com));
        }

        if self.nullifiers.contains_key(&trans.nullifier) {
            return Err(TransactionInvalid::NullifierAlreadyPresent(trans.nullifier));
        } else if Self::on_whitelist(
            whitelist,
            &trans.contract_address.as_ref().map(|x| *x.deref()),
        ) {
            self.nullifiers = self.nullifiers.insert(trans.nullifier, ());
        }

        self.coin_coms_set = self.coin_coms_set.insert(trans.coin_com, ());
        let first_free = self.first_free;
        self.coin_coms = self
            .coin_coms
            .try_update_hash(
                first_free,
                trans.coin_com.0,
                trans.contract_address.as_ref().map(|x| Sp::new(*x.deref())),
            )
            .map_err(TransactionInvalid::MerkleTreeError)?;

        if !Self::on_whitelist(
            whitelist,
            &trans.contract_address.as_ref().map(|x| *x.deref()),
        ) {
            self.coin_coms = self.coin_coms.collapse(first_free, first_free);
        }

        self.first_free = first_free + 1;
        Ok((self, trans.coin_com, first_free)) // Different from the spec because I'm referring to the pre-plus-1 value
    }

    #[instrument(skip(whitelist))]
    fn on_whitelist(
        whitelist: &Option<Map<ContractAddress, ()>>,
        contract: &Option<ContractAddress>,
    ) -> bool {
        match (whitelist, contract) {
            (Some(list), Some(addr)) => list.contains_key(addr),
            // If we have a contract whitelist, the assumption is that we're
            // tracking a contract, *not* a user state!
            (Some(_), None) => false,
            (None, None) | (None, Some(_)) => true,
        }
    }

    #[instrument(skip(self, offer, whitelist))]
    pub fn try_apply<P: Storable<D> + Deserializable>(
        &self,
        offer: &Offer<P, D>,
        whitelist: Option<Map<ContractAddress, ()>>,
    ) -> Result<(Self, Map<Commitment, u64>), TransactionInvalid> {
        let mut com_indicies = Map::new();
        let mut new_st = offer
            .inputs
            .iter_deref()
            .try_fold(self.clone(), |state, inp| {
                state.apply_input(inp.clone(), &whitelist)
            })?;
        (new_st, com_indicies) = offer.outputs.iter_deref().try_fold(
            (new_st, com_indicies),
            |(state, indicies), output| {
                let (state, com, index) = state.apply_output(output.clone(), &whitelist)?;
                Ok((state, indicies.insert(com, index)))
            },
        )?;
        (new_st, com_indicies) = offer.transient.iter_deref().try_fold(
            (new_st, com_indicies),
            |(state, indicies), trans| {
                let (state, com, index) = state.apply_transient(trans.clone(), &whitelist)?;
                Ok((state, indicies.insert(com, index)))
            },
        )?;
        Ok((new_st, com_indicies))
    }

    pub fn filter(
        &self,
        filter: &[ContractAddress],
    ) -> MerkleTree<Option<Sp<ContractAddress, D>>, D> {
        let retained_indices: Vec<u64> = self
            .coin_coms
            .iter_aux()
            .filter(|(_index, (_hash, opt_aux))| match opt_aux {
                Some(aux) => filter.contains(aux),
                None => false,
            })
            .map(|(index, ..)| index)
            .collect();
        let mut tree = self.coin_coms.clone();
        let mut p = 0;
        for i in retained_indices {
            if i > 0 {
                tree = tree.collapse(p, i - 1);
            }
            if i < u64::MAX {
                p = i + 1;
            }
        }
        if self.first_free > 0 {
            tree.collapse(p, self.first_free - 1)
        } else {
            tree
        }
    }

    pub fn post_block_update(&self, tblock: Timestamp) -> Self {
        let mut new_st = self.clone();
        new_st.coin_coms = new_st.coin_coms.rehash();
        new_st.past_roots = new_st.past_roots.insert(
            tblock,
            new_st
                .coin_coms
                .root()
                .expect("rehashed tree must have root"),
        );
        new_st.past_roots = new_st
            .past_roots
            .filter(tblock - (Duration::from_secs(3600)));

        new_st
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::{DB, Delta};
    use crate::{Input, Offer, Output};
    use coin_structure::coin::{Info as CoinInfo, ShieldedTokenType, TokenType};
    use coin_structure::contract::ContractAddress;
    use coin_structure::transfer::Recipient;
    use rand::rngs::ThreadRng;
    use rand::{CryptoRng, Rng};
    use storage::db::InMemoryDB;

    #[test]
    fn test_filtered_spend() {
        fn insert_dummy_outputs<R: Rng + CryptoRng, D: DB>(
            rng: &mut R,
            mut state: State<D>,
            n: usize,
        ) -> State<D> {
            for _ in 0..n {
                let (type_, value) = (rng.r#gen(), rng.r#gen());
                let delta = Delta {
                    token_type: type_,
                    value: value as i128,
                };
                let info = CoinInfo {
                    nonce: rng.r#gen(),
                    type_,
                    value,
                };
                let cpk = coin_structure::coin::PublicKey(rng.r#gen());
                let output = Output::new(rng, &info, None, &cpk, None).unwrap();
                state = state
                    .try_apply(
                        &Offer {
                            inputs: vec![].into(),
                            outputs: vec![output].into(),
                            transient: vec![].into(),
                            deltas: vec![delta].into(),
                        },
                        None,
                    )
                    .unwrap()
                    .0
                    .post_block_update(Default::default());
            }
            state
        }
        let mut state = State::<InMemoryDB>::new();
        let mut rng = rand::thread_rng();
        let coin = CoinInfo {
            nonce: rng.r#gen(),
            type_: rng.r#gen(),
            value: 500,
        };
        let addr = ContractAddress::default();
        state = insert_dummy_outputs(&mut rng, state, 25);
        let output = Output::new_contract_owned(&mut rng, &coin, None, addr).unwrap();
        let (new_state, indices) = state
            .try_apply(
                &Offer {
                    inputs: vec![].into(),
                    outputs: vec![output].into(),
                    transient: vec![].into(),
                    deltas: vec![Delta {
                        token_type: rng.r#gen(),
                        value: 500,
                    }]
                    .into(),
                },
                None,
            )
            .unwrap();
        state = new_state.post_block_update(Default::default());
        state = insert_dummy_outputs(&mut rng, state, 25);
        let qcoin = coin.qualify(
            *indices
                .get(&coin.commitment(&Recipient::Contract(addr)))
                .unwrap(),
        );
        Input::new_contract_owned(&mut rng, &qcoin, None, addr, &state.filter(&[addr])).unwrap();
    }
}
