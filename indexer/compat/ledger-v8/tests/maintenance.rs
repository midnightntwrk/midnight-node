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

#![deny(warnings)]

#[cfg(feature = "proving")]
use base_crypto::rng::SplittableRng;
use base_crypto::signatures::{Signature, SigningKey};
use coin_structure::contract::ContractAddress;
#[cfg(feature = "proving")]
use midnight_ledger::test_utilities::{Resolver, test_resolver, tx_prove};
use midnight_ledger::test_utilities::{TestState, test_intents};
use midnight_ledger::{
    error::{MalformedTransaction, TransactionInvalid},
    semantics::TransactionResult,
    structure::{
        ContractDeploy, ContractOperationVersion, ContractOperationVersionedVerifierKey,
        MaintenanceUpdate, ProofPreimageMarker, SingleUpdate, Transaction,
    },
    verify::WellFormedStrictness,
};
use onchain_runtime::state::{
    ContractMaintenanceAuthority, ContractOperation, ContractState, EntryPointBuf, StateValue,
};
use rand::{CryptoRng, Rng, SeedableRng, rngs::StdRng};
use serialize::{Deserializable, tagged_deserialize, tagged_serialize};
use storage::db::{DB, InMemoryDB};
use storage::storage::HashMap;
use transient_crypto::commitment::PedersenRandomness;
use transient_crypto::proofs::VerifierKey;

fn update_tx<R: Rng + CryptoRng, D: DB>(
    rng: &mut R,
    update: MaintenanceUpdate<D>,
    state: &TestState<D>,
) -> Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D> {
    Transaction::new(
        "local-test",
        test_intents(rng, Vec::new(), vec![update], Vec::new(), state.time),
        None,
        HashMap::new(),
    )
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn schnorr_validity() {
    use lazy_static::lazy_static;

    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);
    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;

    let authority = ContractMaintenanceAuthority {
        committee: vec![],
        threshold: 0,
        counter: 0,
    };
    let cstate = ContractState::new(StateValue::Null, crate::HashMap::new(), authority.clone());
    let deploy = ContractDeploy::new(&mut rng, cstate);
    let addr = deploy.address();
    let deploy_tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        Transaction::new(
            "local-test",
            test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
            None,
            HashMap::new(),
        );
    state.assert_apply(&deploy_tx, strictness);

    let next_authority = ContractMaintenanceAuthority {
        committee: vec![],
        threshold: 1,
        counter: 1,
    };

    let update = MaintenanceUpdate::new(
        addr,
        vec![SingleUpdate::ReplaceAuthority(next_authority.clone())],
        0,
    );

    let tx = Transaction::new(
        "local-test",
        test_intents(
            &mut rng,
            Vec::new(),
            vec![update.clone()],
            Vec::new(),
            state.time,
        ),
        None,
        HashMap::new(),
    );
    lazy_static! {
        static ref RESOLVER: Resolver = test_resolver("");
    }
    let mut tx = tx_prove(rng.split(), &tx, &RESOLVER).await.unwrap();
    let mut tx_ser = Vec::new();
    tagged_serialize(&tx, &mut tx_ser).unwrap();
    tx = tagged_deserialize(&mut &tx_ser[..]).unwrap();
    let mut tx_ser2 = Vec::new();
    tagged_serialize(&tx, &mut tx_ser2).unwrap();
    assert_eq!(tx_ser, tx_ser2);
    assert!(
        tx.well_formed(&state.ledger, strictness, state.time)
            .is_ok()
    );
}

#[test]
fn maintenance() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);
    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    let fake_vk = VerifierKey::deserialize(&mut &b"\x00\x00\x00\x00"[..], 0).unwrap();

    let committee_sks: Vec<_> = (0..4).map(|_| SigningKey::sample(&mut rng)).collect();
    let committee_pks = committee_sks
        .iter()
        .map(SigningKey::verifying_key)
        .collect::<Vec<_>>();
    let authority = ContractMaintenanceAuthority {
        committee: committee_pks.clone(),
        threshold: 2,
        counter: 0,
    };
    let cstate = ContractState::new(
        StateValue::Null,
        HashMap::new().insert(
            b"foo"[..].to_owned().into(),
            ContractOperation::new(Some(fake_vk.clone())),
        ),
        authority.clone(),
    );
    let deploy = ContractDeploy::new(&mut rng, cstate);
    let addr = deploy.address();
    let deploy_tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        Transaction::new(
            "local-test",
            test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
            None,
            HashMap::new(),
        );
    state.assert_apply(&deploy_tx, strictness);

    let next_authority = ContractMaintenanceAuthority {
        committee: committee_pks,
        threshold: 3,
        counter: 1,
    };

    let update = MaintenanceUpdate::new(
        addr,
        vec![SingleUpdate::ReplaceAuthority(next_authority.clone())],
        0,
    );

    // Insufficient signatures
    // Then replace with sufficient signatures
    {
        let data = update.data_to_sign();
        let mut update = update
            .clone()
            .add_signature(1, committee_sks[1].sign(&mut rng, &data));

        let tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
            Transaction::new(
                "local-test",
                test_intents(
                    &mut rng,
                    Vec::new(),
                    vec![update.clone()],
                    Vec::new(),
                    state.time,
                ),
                None,
                HashMap::new(),
            );
        assert!(matches!(
            dbg!(tx.well_formed(&state.ledger, strictness, state.time)),
            Err(MalformedTransaction::ThresholdMissed { .. })
        ));

        update = update.add_signature(3, committee_sks[3].sign(&mut rng, &data));
        update = update.add_signature(2, committee_sks[2].sign(&mut rng, &data));

        let mut tx = update_tx(&mut rng, update.clone(), &state);
        let mut tx_ser = Vec::new();
        tagged_serialize(&tx, &mut tx_ser).unwrap();
        tx = tagged_deserialize(&mut &tx_ser[..]).unwrap();
        let mut state2 = state.clone();
        state2.assert_apply(&tx, strictness);
        assert_eq!(
            state2
                .ledger
                .index(addr)
                .as_ref()
                .unwrap()
                .maintenance_authority,
            next_authority.clone()
        );
    }

    // Targeting the wrong contract address
    {
        let mut update = update.clone();
        update.address = ContractAddress(rng.r#gen());
        let data = update.data_to_sign();
        for i in 0..2 {
            update = update.add_signature(i, committee_sks[i as usize].sign(&mut rng, &data));
        }
        let tx = update_tx(&mut rng, update.clone(), &state);
        assert!(matches!(
            dbg!(tx.well_formed(&state.ledger, strictness, state.time)),
            Err(MalformedTransaction::ContractNotPresent { .. })
        ));
    }

    // Signing the wrong data
    {
        let mut data = update.data_to_sign();
        data[0] = 0;
        let mut update = update.clone();
        for i in 0..2 {
            update = update.add_signature(i, committee_sks[i as usize].sign(&mut rng, &data));
        }
        let tx = update_tx(&mut rng, update.clone(), &state);
        assert!(matches!(
            dbg!(tx.well_formed(&state.ledger, strictness, state.time)),
            Err(MalformedTransaction::InvalidCommitteeSignature { .. })
        ));
    }

    // Signing the wrong keys (key invalid)
    {
        let data = update.data_to_sign();
        let mut update = update.clone();
        for i in 0..2 {
            let key = SigningKey::sample(&mut rng);
            update = update.add_signature(i, key.sign(&mut rng, &data));
        }
        let tx = update_tx(&mut rng, update.clone(), &state);
        assert!(matches!(
            dbg!(tx.well_formed(&state.ledger, strictness, state.time)),
            Err(MalformedTransaction::InvalidCommitteeSignature { .. })
        ));
    }

    // Signing the wrong keys (key ID not in committee)
    {
        let data = update.data_to_sign();
        let mut update = update.clone();
        for i in 0..2 {
            update = update.add_signature(i + 10, committee_sks[i as usize].sign(&mut rng, &data));
        }
        let tx = update_tx(&mut rng, update.clone(), &state);
        assert!(matches!(
            dbg!(tx.well_formed(&state.ledger, strictness, state.time)),
            Err(MalformedTransaction::KeyNotInCommittee { .. })
        ));
    }

    // Signing the wrong keys (signed with valid committe key, for the wrong ID)
    {
        let data = update.data_to_sign();
        let mut update = update.clone();
        for i in 0..2 {
            update = update.add_signature(i, committee_sks[3 - i as usize].sign(&mut rng, &data));
        }
        let tx = update_tx(&mut rng, update.clone(), &state);
        dbg!(&tx);
        assert!(matches!(
            dbg!(tx.well_formed(&state.ledger, strictness, state.time)),
            Err(MalformedTransaction::InvalidCommitteeSignature { .. })
        ));
    }

    // Multi-signing
    {
        let data = update.data_to_sign();
        let mut update = update.clone();
        for _ in 0..2 {
            update = update.add_signature(0, committee_sks[0].sign(&mut rng, &data));
        }
        let tx = update_tx(&mut rng, update.clone(), &state);
        dbg!(&tx);
        assert!(matches!(
            dbg!(tx.well_formed(&state.ledger, strictness, state.time)),
            Err(MalformedTransaction::NotNormalized)
        ));
    }

    // Wrong tx counter
    {
        let mut update = update.clone();
        update.counter = 1;
        let data = update.data_to_sign();
        for i in 0..2 {
            update = update.add_signature(i, committee_sks[i as usize].sign(&mut rng, &data));
        }
        let tx = update_tx(&mut rng, update.clone(), &state);
        dbg!(&tx);
        assert!(matches!(
            dbg!(tx.well_formed(&state.ledger, strictness, state.time)),
            Err(MalformedTransaction::NotNormalized)
        ));
    }

    // remove + insert
    {
        let mut update = update.clone();
        update.updates = vec![
            SingleUpdate::VerifierKeyRemove(
                b"foo"[..].to_owned().into(),
                ContractOperationVersion::V3,
            ),
            SingleUpdate::VerifierKeyInsert(
                b"bar"[..].to_owned().into(),
                ContractOperationVersionedVerifierKey::V3(fake_vk.clone()),
            ),
            SingleUpdate::VerifierKeyInsert(
                b"baz"[..].to_owned().into(),
                ContractOperationVersionedVerifierKey::V3(fake_vk.clone()),
            ),
            SingleUpdate::VerifierKeyRemove(
                b"baz"[..].to_owned().into(),
                ContractOperationVersion::V3,
            ),
        ]
        .into();
        let data = update.data_to_sign();
        for i in 0..2 {
            update = update.add_signature(i, committee_sks[i as usize].sign(&mut rng, &data));
        }
        let tx = update_tx(&mut rng, update.clone(), &state);
        dbg!(&tx);
        assert!(dbg!(tx.well_formed(&state.ledger, strictness, state.time)).is_ok());
        let mut state2 = state.clone();
        state2.assert_apply(&tx, strictness);
        let cstate = state2.ledger.index(addr).unwrap();
        assert_eq!(cstate.maintenance_authority.threshold, authority.threshold);
        assert_eq!(cstate.maintenance_authority.committee, authority.committee);
        assert_eq!(cstate.maintenance_authority.counter, authority.counter + 1);
        assert!(
            cstate
                .operations
                .get(&EntryPointBuf(b"foo"[..].to_owned()))
                .is_none()
        );
        assert!(
            cstate
                .operations
                .get(&EntryPointBuf(b"bar"[..].to_owned()))
                .is_some()
        );
        assert!(
            cstate
                .operations
                .get(&EntryPointBuf(b"baz"[..].to_owned()))
                .is_none()
        );
    }

    // remove not present
    {
        let mut update = update.clone();
        update.updates = vec![SingleUpdate::VerifierKeyRemove(
            b"bar"[..].to_owned().into(),
            ContractOperationVersion::V3,
        )]
        .into();
        let data = update.data_to_sign();
        for i in 0..2 {
            update = update.add_signature(i, committee_sks[i as usize].sign(&mut rng, &data));
        }
        let tx = update_tx(&mut rng, update.clone(), &state);
        dbg!(&tx);
        let res: Result<_, TransactionInvalid<InMemoryDB>> =
            match dbg!(state.apply(&tx, strictness)) {
                Ok(TransactionResult::PartialSuccess(hash_map, ..)) => {
                    let cloned_map = hash_map.clone();
                    cloned_map.get(&1).unwrap().clone()
                }
                _ => panic!("unexpected result structure from state.apply"),
            };
        assert!(matches!(
            dbg!(res),
            Err(TransactionInvalid::VerifierKeyNotFound(..))
        ));
    }

    // insert already present
    {
        let mut update = update.clone();
        update.updates = vec![SingleUpdate::VerifierKeyInsert(
            b"foo"[..].to_owned().into(),
            ContractOperationVersionedVerifierKey::V3(fake_vk.clone()),
        )]
        .into();
        let data = update.data_to_sign();
        for i in 0..2 {
            update = update.add_signature(i, committee_sks[i as usize].sign(&mut rng, &data));
        }
        let tx = update_tx(&mut rng, update.clone(), &state);
        dbg!(&tx);
        let res: Result<_, TransactionInvalid<InMemoryDB>> = match state.apply(&tx, strictness) {
            Ok(TransactionResult::PartialSuccess(hash_map, ..)) => {
                let cloned_map = hash_map.clone();
                cloned_map.get(&1).unwrap().clone()
            }
            _ => panic!("unexpected result structure from state.apply"),
        };

        assert!(matches!(
            dbg!(res),
            Err(TransactionInvalid::VerifierKeyAlreadyPresent(..))
        ));
    }
}
