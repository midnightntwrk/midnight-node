use subxt::ext::{codec, subxt_rpcs};

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Request error: {0}")]
	Requests(#[from] subxt_rpcs::Error),

	#[error("Decode error: {0}")]
	Decode(#[from] codec::Error),
}
