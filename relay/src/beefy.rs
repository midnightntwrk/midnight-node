use std::{fmt::Display, fs::File, io::BufReader, path::Path};

use serde::{Deserialize, Serialize};
use subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params};

pub type BeefyKeys = Vec<BeefyKeyInfo>;

/// Used for inserting keys to the keystore
#[derive(Serialize, Deserialize)]
pub struct BeefyKeyInfo {
	/// Secret seed, for inserting beefy key
	suri: String,

	/// The public key of the secret seed (in ECDSA)
	pub_key: String,
}

impl BeefyKeyInfo {
    pub async fn insert_key(self, rpc: &RpcClient) {
        let params = rpc_params!["beef".to_string(), self.suri, self.pub_key.clone()];

        if let Err(e) = rpc.request::<()>("author_insertKey", params).await {
            println!("Warning: failed to insert key({}): {e:?}", self.pub_key);
            return;

        }

        println!("Added beefy key: {}", self.pub_key);
    }
}

pub fn keys_from_file<T: AsRef<Path> + Display >(key_file: T) -> BeefyKeys {
	let file_open_err = format!("failed to read from key_file {key_file}");
	let file_read_err = format!("cannot read beefy keys in key_file {key_file}");

	let key_file = File::open(key_file).expect(&file_open_err);
	let reader = BufReader::new(key_file);

	// Read the JSON contents of the file as an instance of `User`.
	serde_json::from_reader(reader).expect(&file_read_err)
}