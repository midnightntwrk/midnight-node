#[derive(Debug, thiserror::Error)]
pub enum Error {
	// --------- Beefy Keys errors ---------
	#[error("Failed to read keys from {0}")]
	InvalidKeysFile(String),

	#[error("Failed to parse {0}")]
	JsonDecodeError(String),
}
