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

use crate::structure::*;
use base_crypto::data_provider::MidnightDataProvider;
use base_crypto::rng::SplittableRng;
use futures::future::join_all;
use rand::{CryptoRng, Rng};
use serialize::{Deserializable, tagged_deserialize, tagged_serialize};
use std::fs::File;
use std::future::Future;
use std::io::{BufReader, Read};
use std::sync::Arc;
use storage::db::DB;
use transient_crypto::proofs::{
    KeyLocation, ParamsProverProvider, Proof, ProofPreimage, ProverKey, ProvingError, Resolver,
    VerifierKey,
};
use transient_crypto::proofs::{ParamsProver, ProvingKeyMaterial, ProvingProvider};

#[derive(Clone)]
pub struct ZswapResolver(pub MidnightDataProvider);

impl Resolver for ZswapResolver {
    async fn resolve_key(&self, key: KeyLocation) -> std::io::Result<Option<ProvingKeyMaterial>> {
        let file_root = match &*key.0 {
            "midnight/zswap/spend" => {
                concat!("zswap/", midnight_ledger_static::version!(), "/spend")
            }
            "midnight/zswap/output" => {
                concat!("zswap/", midnight_ledger_static::version!(), "/output")
            }
            "midnight/zswap/sign" => {
                concat!("zswap/", midnight_ledger_static::version!(), "/sign")
            }
            _ => return Ok(None),
        };
        let read_to_vec = |mut read: BufReader<File>| {
            let mut buf = Vec::new();
            read.read_to_end(&mut buf)?;
            Ok::<_, std::io::Error>(buf)
        };
        let prover_key = read_to_vec(
            self.0
                .get_file(
                    &format!("{file_root}.prover"),
                    &format!("failed to find built-in zswap prover key {file_root}.prover"),
                )
                .await?,
        )?;
        let verifier_key = read_to_vec(
            self.0
                .get_file(
                    &format!("{file_root}.verifier"),
                    &format!("failed to find built-in zswap verifier key {file_root}.verifier"),
                )
                .await?,
        )?;
        let ir_source = read_to_vec(
            self.0
                .get_file(
                    &format!("{file_root}.bzkir"),
                    &format!("failed to find built-in zswap IR {file_root}.bzkir"),
                )
                .await?,
        )?;
        Ok(Some(ProvingKeyMaterial {
            prover_key,
            verifier_key,
            ir_source,
        }))
    }
}

impl ParamsProverProvider for ZswapResolver {
    async fn get_params(&self, k: u8) -> std::io::Result<ParamsProver> {
        self.0.get_params(k).await
    }
}

impl AuthorizedClaim<ProofPreimage> {
    pub async fn prove(
        &self,
        prover: impl ProvingProvider,
    ) -> Result<AuthorizedClaim<Proof>, ProvingError> {
        Ok(AuthorizedClaim {
            coin: self.coin,
            recipient: self.recipient,
            proof: Arc::new(prover.prove(&self.proof, None).await?),
        })
    }
}

impl<D: DB> Offer<ProofPreimage, D> {
    pub async fn prove(
        &self,
        mut prover: impl ProvingProvider,
        segment_id: u16,
    ) -> Result<(u16, Offer<Proof, D>), ProvingError> {
        let inputs = Vec::from(self.inputs.clone());
        let outputs = Vec::from(self.outputs.clone());
        let transient = Vec::from(self.transient.clone());
        let (inputs, outputs, transient) = futures::join!(
            join_all(inputs.iter().map(|i| i.prove(prover.split()))),
            join_all(outputs.iter().map(|o| o.prove(prover.split()))),
            join_all(transient.iter().map(|io| io.prove(prover.split())))
        );
        let mut offer = Offer {
            inputs: inputs.into_iter().collect::<Result<_, _>>()?,
            outputs: outputs.into_iter().collect::<Result<_, _>>()?,
            transient: transient.into_iter().collect::<Result<_, _>>()?,
            deltas: self.deltas.clone(),
        };
        offer.normalize();
        Ok((segment_id, offer))
    }
}

impl<D: DB> Input<ProofPreimage, D> {
    pub async fn prove(
        &self,
        prover: impl ProvingProvider,
    ) -> Result<Input<Proof, D>, ProvingError> {
        Ok(Input {
            nullifier: self.nullifier,
            value_commitment: self.value_commitment,
            contract_address: self.contract_address.clone(),
            merkle_tree_root: self.merkle_tree_root,
            proof: Arc::new(prover.prove(&self.proof, None).await?),
        })
    }
}

impl<D: DB> Output<ProofPreimage, D> {
    pub async fn prove(
        &self,
        prover: impl ProvingProvider,
    ) -> Result<Output<Proof, D>, ProvingError> {
        Ok(Output {
            coin_com: self.coin_com,
            value_commitment: self.value_commitment,
            contract_address: self.contract_address.clone(),
            ciphertext: self.ciphertext.clone(),
            proof: Arc::new(prover.prove(&self.proof, None).await?),
        })
    }
}

impl<D: DB> Transient<ProofPreimage, D> {
    pub async fn prove(
        &self,
        mut prover: impl ProvingProvider,
    ) -> Result<Transient<Proof, D>, ProvingError> {
        let (proof_input, proof_output) = futures::join!(
            prover.split().prove(&self.proof_input, None),
            prover.split().prove(&self.proof_output, None),
        );
        Ok(Transient {
            nullifier: self.nullifier,
            coin_com: self.coin_com,
            value_commitment_input: self.value_commitment_input,
            value_commitment_output: self.value_commitment_output,
            contract_address: self.contract_address.clone(),
            ciphertext: self.ciphertext.clone(),
            proof_input: Arc::new(proof_input?),
            proof_output: Arc::new(proof_output?),
        })
    }
}

#[cfg(test)]
mod tests {
    use base_crypto::data_provider;
    use coin_structure::transfer::Recipient;
    use rand::{SeedableRng, rngs::StdRng};
    use storage::db::InMemoryDB;
    use transient_crypto::merkle_tree::MerkleTree;
    use zkir_v2::{Instruction, IrSource, LocalProvingProvider};

    use super::*;

    #[test]
    fn test_pi_lengths() {
        fn count_pis(ir: &str) -> usize {
            use serialize::Deserializable;
            use std::fs::File;
            use std::path::PathBuf;
            let file = PathBuf::from("./static").join(ir).with_extension("bzkir");
            let ir = tagged_deserialize::<IrSource>(&mut File::open(file).unwrap()).unwrap();
            ir.instructions
                .iter()
                .filter_map(|ins| match ins {
                    Instruction::PiSkip { count, .. } => Some(*count as usize),
                    _ => None,
                })
                .sum::<usize>()
                + 1
        }
        assert_eq!(AUTHORIZED_CLAIM_PIS, count_pis("sign"));
        assert_eq!(OUTPUT_PIS, count_pis("output"));
        assert_eq!(INPUT_PIS, count_pis("spend"));
    }

    #[tokio::test]
    async fn test_proof_sizes() {
        use coin_structure::coin::{Info as CoinInfo, QualifiedInfo as QualifiedCoinInfo};
        let mut rng = StdRng::seed_from_u64(0x42);
        let resolver = ZswapResolver(
            MidnightDataProvider::new(
                data_provider::FetchMode::Synchronous,
                data_provider::OutputMode::Log,
                ZSWAP_EXPECTED_FILES.to_owned(),
            )
            .unwrap(),
        );
        let mut provider = LocalProvingProvider {
            rng: rng.split(),
            params: &resolver,
            resolver: &resolver,
        };

        let qcoin = QualifiedCoinInfo {
            value: Default::default(),
            type_: Default::default(),
            nonce: rng.r#gen(),
            mt_index: 0,
        };
        let coin = CoinInfo::from(&qcoin);
        let recipient = Recipient::Contract(Default::default());
        let tree = MerkleTree::<(), InMemoryDB>::blank(32)
            .try_update_hash(0, coin.commitment(&recipient).0, ())
            .expect("updating hash on non-collapsed tree should always succeed")
            .rehash();

        let inp =
            Input::new_contract_owned(&mut rng, &qcoin, None, Default::default(), &tree).unwrap();
        let inp_proven = inp.prove(provider.split()).await.unwrap();
        assert_eq!(inp_proven.proof.0.len(), INPUT_PROOF_SIZE);

        let out =
            Output::<_, InMemoryDB>::new_contract_owned(&mut rng, &coin, None, Default::default())
                .unwrap();
        let out_proven = out.prove(provider).await.unwrap();
        assert_eq!(out_proven.proof.0.len(), OUTPUT_PROOF_SIZE);
    }
}
