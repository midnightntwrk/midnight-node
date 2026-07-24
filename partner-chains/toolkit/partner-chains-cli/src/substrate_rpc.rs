use anyhow::anyhow;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::rpc_params;
use parity_scale_codec::{Decode, Encode};
use sp_core::crypto::KeyTypeId;
use std::time::Duration;

/// Runtime API method used to decode the opaque session-keys blob returned by
/// `author_rotateKeys`. Present in every Substrate runtime implementing
/// `sp_session::SessionKeys`.
const DECODE_SESSION_KEYS_METHOD: &str = "SessionKeys_decode_session_keys";

/// Request to the RPC endpoint of a running partner chain node.
#[derive(Debug, Eq, PartialEq)]
pub enum SubstrateRpcRequest {
	/// `author_rotateKeys`: generates a new set of session keys in the node's keystore
	/// and returns the SCALE-encoded concatenation of their public keys.
	AuthorRotateKeys,
	/// `state_call("SessionKeys_decode_session_keys")`: decodes an opaque session-keys
	/// blob into (key type id, public key bytes) pairs using the runtime itself, so the
	/// key set is never hardcoded in the toolkit.
	DecodeSessionKeys { encoded: Vec<u8> },
}

/// Response from the RPC endpoint of a running partner chain node.
#[derive(Debug, Eq, PartialEq)]
pub enum SubstrateRpcResponse {
	/// Opaque SCALE-encoded session keys blob returned by `author_rotateKeys`.
	RotatedKeys(Vec<u8>),
	/// Session keys decoded by the runtime, or [None] if the blob could not be decoded.
	DecodedKeys(Option<Vec<(KeyTypeId, Vec<u8>)>>),
}

/// Performs a blocking request against the RPC endpoint of a running partner chain node.
pub fn substrate_rpc(
	url: &str,
	timeout: Duration,
	req: SubstrateRpcRequest,
) -> anyhow::Result<SubstrateRpcResponse> {
	let tokio_runtime = tokio::runtime::Runtime::new().map_err(|e| anyhow!(e))?;
	tokio_runtime.block_on(async {
		let client = HttpClientBuilder::default()
			.request_timeout(timeout)
			.build(url)
			.map_err(|e| anyhow!("Failed to connect to the partner chain node at {url}: {e}"))?;
		match req {
			SubstrateRpcRequest::AuthorRotateKeys => {
				let response: String = client
					.request("author_rotateKeys", rpc_params![])
					.await
					.map_err(|e| map_rpc_error("author_rotateKeys", url, e))?;
				Ok(SubstrateRpcResponse::RotatedKeys(decode_hex_bytes(&response)?))
			},
			SubstrateRpcRequest::DecodeSessionKeys { encoded } => {
				let call_args = encode_decode_session_keys_args(&encoded);
				let response: String = client
					.request("state_call", rpc_params![DECODE_SESSION_KEYS_METHOD, call_args])
					.await
					.map_err(|e| map_rpc_error("state_call", url, e))?;
				let bytes = decode_hex_bytes(&response)?;
				let decoded = <Option<Vec<(Vec<u8>, KeyTypeId)>>>::decode(&mut &bytes[..])
					.map_err(|e| {
						anyhow!("Failed to decode session keys returned by the node: {e}")
					})?;
				Ok(SubstrateRpcResponse::DecodedKeys(
					decoded.map(|keys| keys.into_iter().map(|(bytes, id)| (id, bytes)).collect()),
				))
			},
		}
	})
}

/// `state_call` expects the SCALE-encoded arguments of the runtime function, which for
/// `decode_session_keys(encoded: Vec<u8>)` is the compact-length-prefixed blob, not the
/// raw blob itself.
fn encode_decode_session_keys_args(encoded_keys: &[u8]) -> String {
	format!("0x{}", hex::encode(encoded_keys.to_vec().encode()))
}

fn decode_hex_bytes(response: &str) -> anyhow::Result<Vec<u8>> {
	hex::decode(response.trim_start_matches("0x"))
		.map_err(|e| anyhow!("Node returned malformed hex response '{response}': {e}"))
}

fn map_rpc_error(method: &str, url: &str, err: jsonrpsee::core::ClientError) -> anyhow::Error {
	let msg = err.to_string();
	if msg.to_lowercase().contains("unsafe") {
		anyhow!(
			"'{method}' RPC call to {url} was rejected as unsafe: {msg}. \
			 The node allows unsafe RPC methods for localhost connections by default; \
			 for other addresses it must be started with '--rpc-methods=unsafe'."
		)
	} else {
		anyhow!("'{method}' RPC call to {url} failed: {msg}")
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use parity_scale_codec::Encode;

	#[test]
	fn state_call_args_are_the_scale_encoded_blob() {
		// fixture: the raw blob must be length-prefixed like `Vec<u8>::encode`
		let blob = vec![1u8, 2, 3, 4];
		assert_eq!(
			encode_decode_session_keys_args(&blob),
			format!("0x{}", hex::encode(blob.encode()))
		);
		// 4-element Vec<u8> encodes as compact-length 0x10 followed by the bytes
		assert_eq!(encode_decode_session_keys_args(&[1, 2, 3, 4]), "0x1001020304");
	}

	#[test]
	fn decodes_session_keys_state_call_response() {
		let decoded: Option<Vec<(Vec<u8>, KeyTypeId)>> =
			Some(vec![([1u8; 32].to_vec(), KeyTypeId(*b"aura"))]);
		let bytes = decoded.encode();
		let round_tripped = <Option<Vec<(Vec<u8>, KeyTypeId)>>>::decode(&mut &bytes[..]).unwrap();
		assert_eq!(round_tripped, decoded);
	}
}
