use std::{fs::File, io::BufReader};

use subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params};

use crate::Error;

const BEEFY_KEY_TYPE: &str = "beef";

/// Used for inserting keys to the keystore
#[derive(serde::Serialize, serde::Deserialize)]
pub struct BeefyKeyInfo {
	/// Secret seed, for inserting beefy key
	suri: String,

	/// The public key of the secret seed (in ECDSA)
	pub_key: String,

	/// The node url, where to insert the keys
	node_url: String,
}

/// Reads keys from the given file path, and insert them to the chain
pub async fn read_and_insert_to_chain(key_file_path: &str) -> Result<(), Error> {
	// reading keys from file
	let beefy_key_infos = keys_from_file(key_file_path)?;

	// insert each key to keystore of each respective urls
	for key_info in beefy_key_infos {
		key_info.insert_key().await;
	}

	Ok(())
}

impl BeefyKeyInfo {
	/// Insert the beefy key into the chain, given the url
	async fn insert_key(self) {
		match RpcClient::from_url(&self.node_url).await {
			Ok(rpc) => self.insert_key_query(rpc).await,
			Err(e) => println!("Warning: Failed to Connect to {}: {e:#?}", self.node_url),
		}
	}

	/// The actual query to insert the beefy key into the chain
	async fn insert_key_query(self, rpc: RpcClient) {
		let params = rpc_params![BEEFY_KEY_TYPE.to_string(), self.suri, self.pub_key.clone()];

		if let Err(e) = rpc.request::<()>("author_insertKey", params).await {
			println!("Warning: Failed to insert key({}): {e:?}", self.pub_key);
			return;
		}

		println!("Added beefy key: {} to {}", self.pub_key, self.node_url);
	}
}

/// Read beefy keys from the given file
fn keys_from_file(key_file: &str) -> Result<Vec<BeefyKeyInfo>, Error> {
	let file = File::open(key_file).map_err(|e| {
		println!("WARNING: {e:#?}");
		Error::InvalidKeysFile(key_file.to_string())
	})?;

	let reader = BufReader::new(file);

	// Read the JSON contents of the file as an instance of `User`.
	serde_json::from_reader(reader).map_err(|e| {
		println!("WARNING: {e:#?}");
		Error::JsonDecodeError(key_file.to_string())
	})
}

#[cfg(test)]
mod test {
	use crate::beefy_keys::keys_from_file;

	#[test]
	fn test_beefy_keys_file() {
		// get sample data
		let beefy_keys_file = "test-data/beefy-keys.json";

		let beefy_keys = keys_from_file(beefy_keys_file).expect("Failed to get beefykeyinfo");

		assert_eq!(beefy_keys.len(), 2);

		assert_eq!(beefy_keys[0].suri, "//Alice");
		assert_eq!(beefy_keys[1].suri, "//Bob");

		assert_eq!(beefy_keys[0].node_url, "ws://localhost:9933");
		assert_eq!(beefy_keys[1].node_url, "ws://localhost:9934");
	}
}
