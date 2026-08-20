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

use base_crypto::hash::{PERSISTENT_HASH_BYTES, PersistentHashWriter};
use base_crypto::repr::MemWrite;
use coin_structure::coin::{
    self, Commitment, Info as CoinInfo, Nullifier, QualifiedInfo as QualifiedCoinInfo,
};
use coin_structure::transfer::{Recipient, SenderEvidence};
use rand::{CryptoRng, Rng};
use serialize::{Deserializable, Serializable, Tagged};
use storage::storage::Map;
use storage::storage::default_storage;
use transient_crypto::encryption;
use transient_crypto::merkle_tree::{self, MerkleTree, MerkleTreeCollapsedUpdate};
use transient_crypto::proofs::ProofPreimage;
use transient_crypto::repr::FieldRepr;

use crate::ZSWAP_TREE_HEIGHT;
use crate::error::OfferCreationFailed;
use crate::structure::*;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Seed([u8; 32]);

impl Debug for Seed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "<wallet seed>")
    }
}

impl From<[u8; 32]> for Seed {
    fn from(value: [u8; 32]) -> Self {
        Seed(value)
    }
}

impl Seed {
    pub fn random<T: Rng + CryptoRng>(rng: &mut T) -> Seed {
        let mut out: [u8; 32] = [0; 32];
        rng.fill_bytes(&mut out);
        Seed(out)
    }

    pub fn derive_coin_secret_key(self: &Seed) -> coin::SecretKey {
        let domain_separator = b"midnight:csk";
        let mut hash_writer = PersistentHashWriter::new();
        MemWrite::write(&mut hash_writer, domain_separator);
        MemWrite::write(&mut hash_writer, &self.0);
        let hash = hash_writer.finalize();
        coin::SecretKey(hash)
    }

    pub fn derive_encryption_secret_key(self: &Seed) -> encryption::SecretKey {
        const DOMAIN_SEPARATOR: &[u8; 12] = b"midnight:esk";
        const NUMBER_OF_BYTES: usize = 64;
        let mut raw_bytes = self.sample_bytes(NUMBER_OF_BYTES, DOMAIN_SEPARATOR);
        let mut raw_bytes_arr: [u8; 64] = raw_bytes.clone().try_into().unwrap();

        raw_bytes.zeroize();
        let res = encryption::SecretKey::from_uniform_bytes(&raw_bytes_arr);
        raw_bytes_arr.zeroize();
        res
    }

    pub fn sample_bytes(&self, no_of_bytes: usize, domain_separator: &[u8]) -> Vec<u8> {
        let hash_bytes = PERSISTENT_HASH_BYTES;
        let rounds = no_of_bytes.div_ceil(hash_bytes);
        let mut res: Vec<u8> = Vec::new();
        for round in 0..rounds {
            let mut outer_writer = PersistentHashWriter::new();
            MemWrite::write(&mut outer_writer, domain_separator);
            MemWrite::write(&mut outer_writer, &{
                let mut inner_writer = PersistentHashWriter::new();
                MemWrite::write(&mut inner_writer, &((round as u64).to_le_bytes()));
                MemWrite::write(&mut inner_writer, &self.0);
                inner_writer.finalize().0
            });
            let round_hash = outer_writer.finalize();
            let bytes_to_add = hash_bytes.min(no_of_bytes - round * 32);
            res.extend_from_slice(&round_hash.0[0..bytes_to_add])
        }
        res
    }
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKeys {
    pub coin_secret_key: coin::SecretKey,
    pub encryption_secret_key: encryption::SecretKey,
}

impl Debug for SecretKeys {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "<secret keys>")
    }
}

impl From<Seed> for SecretKeys {
    fn from(seed: Seed) -> Self {
        SecretKeys {
            coin_secret_key: seed.derive_coin_secret_key(),
            encryption_secret_key: seed.derive_encryption_secret_key(),
        }
    }
}

impl SecretKeys {
    pub fn from_rng_seed<R: Rng + CryptoRng + ?Sized>(rng: &mut R) -> Self {
        let enc_sk = encryption::SecretKey::new(rng);
        let coin_sk = coin::SecretKey(rng.r#gen());
        SecretKeys {
            coin_secret_key: coin_sk,
            encryption_secret_key: enc_sk,
        }
    }

    pub fn coin_public_key(&self) -> coin::PublicKey {
        self.coin_secret_key.public_key()
    }

    pub fn enc_public_key(&self) -> encryption::PublicKey {
        self.encryption_secret_key.public_key()
    }

    pub fn try_decrypt(&self, msg: &CoinCiphertext) -> Option<CoinInfo> {
        self.encryption_secret_key.decrypt(&msg.clone().into())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use coin_structure::coin::TokenType;
    use hex::FromHex;
    use rand::rngs::OsRng;
    use serde::de::{Error, Unexpected};
    use serde::{Deserialize, Deserializer};

    use super::*;

    impl<'de> Deserialize<'de> for Seed {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s = <String as Deserialize>::deserialize(deserializer)?;
            let as_arr: [u8; 32] = <[u8; 32]>::from_hex(s.clone())
                .map_err(|_err| Error::invalid_value(Unexpected::Str(&s), &"hex string"))?;
            Ok(Seed(as_arr))
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
    struct CoinVectorEntry {
        secretKey: HexArr32,
        publicKey: HexArr32,
    }

    #[allow(non_snake_case)]
    #[derive(Deserialize)]
    struct VectorEntry {
        seed: Seed,
        encryption: EncryptionVectorEntry,
        coin: CoinVectorEntry,
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
        let test_vectors = TestVectors::load();

        for entry in test_vectors.0 {
            let esk_computed = entry.seed.derive_encryption_secret_key();
            let esk_reference =
                encryption::SecretKey::from_repr(&entry.encryption.secretKeyRepr.0).unwrap();
            let intermediate_computed = entry.seed.sample_bytes(64, b"midnight:esk");
            let esk_from_intermediate = encryption::SecretKey::from_uniform_bytes(
                &entry.encryption.secretKeyIntermediateBytes.0,
            );

            println!("Encryption Keys:");
            println!("  seed:                  {:?}", entry.seed.0);
            println!(
                "  intermediate bytes:    {:?}",
                &entry.encryption.secretKeyIntermediateBytes.0
            );
            println!(
                "  intermediate computed: {:?}",
                intermediate_computed.as_slice()
            );
            println!("  computed:              {:?}", esk_computed.repr());
            println!("  reference:             {:?}", esk_reference.repr());
            println!(
                "  reference raw:         {:?}",
                entry.encryption.secretKeyRepr.0
            );
            println!(
                "  from intermediate:     {:?}",
                esk_from_intermediate.repr()
            );

            assert_eq!(
                intermediate_computed.as_slice(),
                entry.encryption.secretKeyIntermediateBytes.0.as_slice(),
                "Intermediate bytes do not match"
            );
            assert_eq!(
                esk_computed.repr(),
                esk_from_intermediate.repr(),
                "Key computed from seed does not match key computed from intermediate bytes"
            );
            assert_eq!(
                esk_computed.repr(),
                esk_reference.repr(),
                "Key computed from seed does not match reference"
            );
            assert_eq!(
                esk_reference.repr(),
                esk_from_intermediate.repr(),
                "Key computed from intermediate bytes does not match reference"
            );
        }
    }

    #[test]
    fn coin_key_derivation_matches_test_vectors() {
        let test_vectors = TestVectors::load();

        for entry in test_vectors.0 {
            let seed = entry.seed;
            let computed_csk = seed.derive_coin_secret_key();
            let computed_cpk = computed_csk.public_key();
            let reference_csk = entry.coin.secretKey;
            let reference_cpk = entry.coin.publicKey;

            println!("Coin keys:");
            println!("  seed:         {:?}", seed.0);
            println!("  reference sk: {:?}", reference_csk.0);
            println!("  computed sk:  {:?}", computed_csk.0.0);
            println!("  reference pk: {:?}", reference_cpk.0);
            println!("  computed pk:  {:?}", computed_cpk.0.0);

            assert_eq!(computed_csk.0.0, reference_csk.0);
            assert_eq!(computed_cpk.0.0, reference_cpk.0);
        }
    }
}
