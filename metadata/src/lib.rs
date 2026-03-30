// Generate an interface that we can use from the node's metadata.
#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.21.0.scale")]
pub mod midnight_metadata_0_21_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.22.0.scale")]
pub mod midnight_metadata_0_22_0 {}

pub use midnight_metadata_0_22_0 as midnight_metadata_latest;

/// Raw SCALE-encoded runtime metadata per version, for version-aware block decoding.
pub const METADATA_0_21_0_BYTES: &[u8] = include_bytes!("../static/midnight_metadata_0.21.0.scale");
pub const METADATA_0_22_0_BYTES: &[u8] = include_bytes!("../static/midnight_metadata_0.22.0.scale");
