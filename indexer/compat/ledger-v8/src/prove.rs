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

use crate::dust::DustResolver;
use crate::error::TransactionProvingError;
use crate::structure::{
    ContractAction, ContractCall, Intent, ProofMarker, ProofPreimageMarker, ProofPreimageVersioned,
    ProofVersioned, SignatureKind, StandardTransaction, Transaction,
};
use base_crypto::cost_model::{CostDuration, RunningCost};
use futures::future::join_all;
use onchain_runtime::cost_model::{CostModel, INITIAL_COST_MODEL};
use onchain_runtime::ops::Op;
use onchain_runtime::transcript::Transcript;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::future::Future;
use std::io;
use std::ops::Deref;
use std::pin::Pin;
use storage::arena::Sp;
use storage::db::DB;
use tokio::runtime::Handle;
use transient_crypto::commitment::PedersenRandomness;
use transient_crypto::commitment::{Pedersen, PureGeneratorPedersen};
use transient_crypto::proofs::{KeyLocation, ParamsProverProvider};
use transient_crypto::proofs::{ProvingError, ProvingKeyMaterial, ProvingProvider};
use zswap::prove::ZswapResolver;

pub type ExternalResolver = Box<
    dyn Fn(
            KeyLocation,
        )
            -> Pin<Box<dyn Future<Output = io::Result<Option<ProvingKeyMaterial>>> + Send + Sync>>
        + Send
        + Sync,
>;

pub struct Resolver {
    pub zswap_resolver: ZswapResolver,
    pub dust_resolver: DustResolver,
    #[allow(clippy::type_complexity)]
    pub external_resolver: ExternalResolver,
}

impl Resolver {
    #[allow(clippy::type_complexity)]
    pub fn new(
        zswap_resolver: ZswapResolver,
        dust_resolver: DustResolver,
        external_resolver: ExternalResolver,
    ) -> Self {
        Resolver {
            zswap_resolver,
            dust_resolver,
            external_resolver,
        }
    }
}

impl ParamsProverProvider for Resolver {
    async fn get_params(&self, k: u8) -> io::Result<transient_crypto::proofs::ParamsProver> {
        self.zswap_resolver.get_params(k).await
    }
}

impl transient_crypto::proofs::Resolver for Resolver {
    async fn resolve_key(&self, key: KeyLocation) -> io::Result<Option<ProvingKeyMaterial>> {
        if let Some(res) = self.zswap_resolver.resolve_key(key.clone()).await? {
            return Ok(Some(res));
        }
        if let Some(res) = self.dust_resolver.resolve_key(key.clone()).await? {
            return Ok(Some(res));
        }
        (self.external_resolver)(key.clone()).await
    }
}

#[instrument(skip(prover, cost_model))]
#[allow(clippy::type_complexity)]
async fn prove_intents<D: DB, S: SignatureKind<D>>(
    intents: &storage::storage::HashMap<
        u16,
        Intent<S, ProofPreimageMarker, PedersenRandomness, D>,
        D,
    >,
    mut prover: impl ProvingProvider,
    cost_model: &CostModel,
) -> Result<
    storage::storage::HashMap<u16, Intent<S, ProofMarker, PedersenRandomness, D>, D>,
    ProvingError,
> {
    let res = join_all(intents.iter().map(|seg_x_intent| {
        let split_prover = prover.split();
        async move {
            seg_x_intent
                .1
                .deref()
                .prove(*seg_x_intent.0.deref(), split_prover, cost_model)
                .await
        }
    }))
    .await
    .into_iter()
    .collect::<Result<storage::storage::HashMap<_, _, _>, _>>()?;

    Ok(res)
}

impl<S: SignatureKind<D>, D: DB> Transaction<S, ProofPreimageMarker, PedersenRandomness, D> {
    /// Mocks proving, producing a 'proven' transaction that, while it will
    /// *not* verify, is accurate for fee computation purposes.
    ///
    /// Due to the variability in proof sizes, this *only* works for
    /// transactions that do not contain unproven contract calls.
    pub fn mock_prove(
        &self,
    ) -> Result<Transaction<S, ProofMarker, PureGeneratorPedersen, D>, TransactionProvingError<D>>
    {
        let tokio_handle = Handle::try_current();
        let _guard = tokio_handle.as_ref().map(Handle::enter);
        let mut proven = futures::executor::block_on(self.prove(MockProver, &INITIAL_COST_MODEL))?
            .seal(StdRng::seed_from_u64(0x00));
        if let Transaction::Standard(ref mut stx) = proven {
            let intents = stx
                .intents
                .iter()
                .map(|segintent| {
                    let mut intent = (*segintent.1).clone();
                    intent.binding_commitment = PureGeneratorPedersen::largest_representable();
                    (*segintent.0, intent)
                })
                .collect();
            stx.intents = intents;
        }
        Ok(proven)
    }

    #[instrument(skip(self, prover, cost_model))]
    pub async fn prove(
        &self,
        mut prover: impl ProvingProvider,
        cost_model: &CostModel,
    ) -> Result<Transaction<S, ProofMarker, PedersenRandomness, D>, TransactionProvingError<D>>
    {
        match self {
            Transaction::Standard(stx) => {
                let coin_vec: Vec<_> = stx.fallible_coins.clone().into_iter().collect();
                let coin_provers: Vec<_> = coin_vec.iter().map(|_| prover.split()).collect();

                let fallible_coins_future =
                    join_all(coin_vec.into_iter().zip(coin_provers.into_iter()).map(
                        |((s_id, o), coin_prover)| async move { o.prove(coin_prover, s_id).await },
                    ));

                let (intents, guaranteed_coins, fallible_coins) = futures::join!(
                    prove_intents(&stx.intents, prover.split(), cost_model),
                    futures::future::OptionFuture::from(
                        stx.guaranteed_coins
                            .as_ref()
                            .map(|o| { o.prove(prover.split(), 0) })
                    ),
                    fallible_coins_future
                );

                Ok(Transaction::Standard(StandardTransaction {
                    network_id: stx.network_id.clone(),
                    intents: intents?,
                    guaranteed_coins: guaranteed_coins
                        .transpose()
                        .map_err(TransactionProvingError::Proving)?
                        .as_ref()
                        .map(|x| Sp::new(x.clone().1)),
                    fallible_coins: fallible_coins
                        .into_iter()
                        .collect::<Result<storage::storage::HashMap<_, _, D>, _>>()
                        .map_err(TransactionProvingError::Proving)?,
                    binding_randomness: stx.binding_randomness,
                }))
            }
            Transaction::ClaimRewards(rewards) => Ok(Transaction::ClaimRewards(rewards.clone())),
        }
    }
}

impl<S: SignatureKind<D>, D: DB> Intent<S, ProofPreimageMarker, PedersenRandomness, D> {
    #[instrument(skip(self, prover, cost_model))]
    #[allow(clippy::type_complexity)]
    pub async fn prove(
        &self,
        segment_id: u16,
        mut prover: impl ProvingProvider,
        cost_model: &CostModel,
    ) -> Result<(u16, Intent<S, ProofMarker, PedersenRandomness, D>), ProvingError> {
        let actions =
            join_all(self.actions.iter_deref().map(|call| {
                call.prove(prover.split(), self.binding_commitment.into(), cost_model)
            }))
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let dust_actions = match self.dust_actions.as_ref() {
            Some(da) => Some(Sp::new(
                da.prove(prover.split(), segment_id, self.binding_commitment.into())
                    .await?,
            )),
            None => None,
        };

        let intent = Intent {
            guaranteed_unshielded_offer: self.guaranteed_unshielded_offer.clone(),
            fallible_unshielded_offer: self.fallible_unshielded_offer.clone(),
            actions: actions.into(),
            dust_actions,
            ttl: self.ttl,
            binding_commitment: self.binding_commitment,
        };

        Ok((segment_id, intent))
    }
}

impl<D: DB> ContractAction<ProofPreimageMarker, D> {
    async fn prove(
        &self,
        prover: impl ProvingProvider,
        binding_commitment: Pedersen,
        cost_model: &CostModel,
    ) -> Result<ContractAction<ProofMarker, D>, TransactionProvingError<D>> {
        use ContractAction::*;
        Ok(match self {
            Call(call) => Call(Sp::new(
                call.prove(prover, binding_commitment, cost_model).await?,
            )),
            Deploy(deploy) => Deploy(deploy.clone()),
            Maintain(upd) => Maintain(upd.clone()),
        })
    }
}

impl<D: DB> ContractCall<ProofPreimageMarker, D> {
    async fn prove(
        &self,
        prover: impl ProvingProvider,
        binding_commitment: Pedersen,
        cost_model: &CostModel,
    ) -> Result<ContractCall<ProofMarker, D>, TransactionProvingError<D>> {
        let active_calls = match &self.proof {
            ProofPreimageVersioned::V2(proof) => prover.check(proof).await?,
        };
        let mut remaining_active_calls = &active_calls[..];

        // Process the transcript programs, inserting noops in inactive segments
        let mut guaranteed_prog = Vec::new();
        let mut fallible_prog = Vec::new();
        for (old_transcript, transcript) in [
            self.guaranteed_transcript
                .as_ref()
                .map(|t| (t.program.iter_deref(), &mut guaranteed_prog)),
            self.fallible_transcript
                .as_ref()
                .map(|t| (t.program.iter_deref(), &mut fallible_prog)),
        ]
        .into_iter()
        .flatten()
        {
            for op in old_transcript {
                while let Some(Some(skip)) = remaining_active_calls.first() {
                    transcript.push(Op::Noop { n: *skip as u32 });
                    remaining_active_calls = &remaining_active_calls[1..];
                }
                transcript.push(op.clone());
                remaining_active_calls = &remaining_active_calls[1..];
            }
            while let Some(Some(skip)) = remaining_active_calls.first() {
                transcript.push(Op::Noop { n: *skip as u32 });
                remaining_active_calls = &remaining_active_calls[1..];
            }
        }
        // Combine adjacent noops, and count their cost.
        let mut guaranteed_noop_gas_cost = CostDuration::ZERO;
        let mut fallible_noop_gas_cost = CostDuration::ZERO;
        for (prog, gas_cost) in [
            (&mut guaranteed_prog, &mut guaranteed_noop_gas_cost),
            (&mut fallible_prog, &mut fallible_noop_gas_cost),
        ]
        .into_iter()
        {
            // Marks the current write head
            let mut i = 0;
            // The current chain of noops
            let mut n = 0;
            // Marks the current read head
            for j in 0..prog.len() {
                match prog[j].clone() {
                    Op::Noop { n: n2 } => n += n2,
                    op => {
                        if n != 0 {
                            prog[i] = Op::Noop { n };
                            *gas_cost +=
                                cost_model.noop_constant + cost_model.noop_coeff_arg * n as u64;
                            i += 1;
                            n = 0;
                        }
                        prog[i] = op;
                        i += 1;
                    }
                }
            }
            if n != 0 {
                prog[i] = Op::Noop { n };
                *gas_cost += cost_model.noop_constant + cost_model.noop_coeff_arg * n as u64;
                i += 1;
            }
            prog.truncate(i);
        }
        let guaranteed_transcript = self.guaranteed_transcript.as_ref().map(|t| Transcript {
            gas: RunningCost {
                compute_time: guaranteed_noop_gas_cost + t.gas.compute_time,
                ..t.gas
            },
            effects: t.effects.clone(),
            program: guaranteed_prog.into(),
            version: t.version.clone(),
        });
        let fallible_transcript = self.fallible_transcript.as_ref().map(|t| Transcript {
            gas: RunningCost {
                compute_time: fallible_noop_gas_cost + t.gas.compute_time,
                ..t.gas
            },
            effects: t.effects.clone(),
            program: fallible_prog.into(),
            version: t.version.clone(),
        });

        let intermediate_call: ContractCall<ProofPreimageMarker, D> = ContractCall {
            address: self.address,
            entry_point: self.entry_point.clone(),
            guaranteed_transcript: guaranteed_transcript.clone().map(Sp::new),
            fallible_transcript: fallible_transcript.clone().map(Sp::new),
            communication_commitment: self.communication_commitment,
            proof: self.proof.clone(),
        };

        let proof = match &self.proof {
            ProofPreimageVersioned::V2(preimage) => ProofVersioned::V2(
                prover
                    .prove(
                        preimage,
                        Some(intermediate_call.binding_input(binding_commitment)),
                    )
                    .await?,
            ),
        };

        // Assemble the final call
        Ok(ContractCall {
            address: self.address,
            entry_point: self.entry_point.clone(),
            guaranteed_transcript: guaranteed_transcript.map(Sp::new),
            fallible_transcript: fallible_transcript.map(Sp::new),
            communication_commitment: self.communication_commitment,
            proof,
        })
    }
}

pub(crate) struct MockProver;

const BUILTIN_KEYS: &[&str] = &[
    "midnight/zswap/spend",
    "midnight/zswap/output",
    "midnight/dust/spend",
];

impl ProvingProvider for MockProver {
    async fn check(
        &self,
        preimage: &transient_crypto::proofs::ProofPreimage,
    ) -> Result<Vec<Option<usize>>, anyhow::Error> {
        if BUILTIN_KEYS.contains(&preimage.key_location.0.as_ref()) {
            Ok(vec![])
        } else {
            anyhow::bail!("cannot mock prove non-builtin circuit")
        }
    }
    async fn prove(
        self,
        preimage: &transient_crypto::proofs::ProofPreimage,
        _overwrite_binding_input: Option<transient_crypto::curve::Fr>,
    ) -> Result<transient_crypto::proofs::Proof, anyhow::Error> {
        let size = match preimage.key_location.0.as_ref() {
            "midnight/zswap/spend" => zswap::INPUT_PROOF_SIZE,
            "midnight/zswap/output" => zswap::OUTPUT_PROOF_SIZE,
            "midnight/dust/spend" => crate::dust::DUST_SPEND_PROOF_SIZE,
            _ => anyhow::bail!("cannot mock prove non-builtin circuit"),
        };
        let nonsense_data = std::iter::repeat([0xde, 0xad, 0xc0, 0xde])
            .flat_map(|v| v.into_iter())
            .take(size)
            .collect::<Vec<_>>();
        let mock_proof = transient_crypto::proofs::Proof(nonsense_data);
        Ok(mock_proof)
    }
    fn split(&mut self) -> Self {
        MockProver
    }
}

#[cfg(test)]
mod tests {
    use base_crypto::{
        data_provider::{self, MidnightDataProvider},
        signatures::Signature,
    };
    use storage::db::InMemoryDB;
    use zswap::ZSWAP_EXPECTED_FILES;

    use crate::{dust::DUST_EXPECTED_FILES, structure::Transaction};

    #[test]
    fn test_mock_proving() {
        let tx = Transaction::<Signature, _, _, InMemoryDB>::new(
            "local-test",
            Default::default(),
            Default::default(),
            Default::default(),
        );
        tx.mock_prove().unwrap();
    }

    #[tokio::test]
    async fn test_mock_proving_tokio() {
        let tx = Transaction::<Signature, _, _, InMemoryDB>::new(
            "local-test",
            Default::default(),
            Default::default(),
            Default::default(),
        );
        tx.mock_prove().unwrap();
    }

    #[tokio::test]
    async fn test_resolver_resolves() {
        if option_env!("NIX_ENFORCE_PURITY").is_some() {
            // We can't run this test in the nix sandbox, due to not having internet!
            // That's okay, we'll test it outside of it.
            return;
        }
        let tmpdir = std::env::temp_dir().join("midnight-resolver-test");
        let files = ZSWAP_EXPECTED_FILES
            .iter()
            .chain(DUST_EXPECTED_FILES.iter())
            .copied()
            .collect::<Vec<_>>();
        let provider = MidnightDataProvider {
            fetch_mode: data_provider::FetchMode::OnDemand,
            base_url: data_provider::BASE_URL.clone(),
            output_mode: data_provider::OutputMode::Log,
            expected_data: files.clone(),
            dir: tmpdir.clone(),
        };
        let mut results = vec![];
        for (name, _, _) in files.iter() {
            results.push(provider.fetch(name).await);
        }
        std::fs::remove_dir_all(tmpdir).unwrap();
        if results.iter().any(|res| res.is_err()) {
            println!("resolver fails to resolve the following keys:");
            for (name, err) in files
                .iter()
                .zip(results.iter())
                .filter_map(|((name, _, _), res)| match res {
                    Ok(_) => None,
                    Err(e) => Some((name, e)),
                })
            {
                println!("  '{name}': {err}");
            }
            panic!();
        }
    }
}
