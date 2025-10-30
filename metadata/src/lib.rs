// Generate an interface that we can use from the node's metadata.
#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.17.0.scale")]
pub mod midnight_metadata_0_17_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.17.1.scale")]
pub mod midnight_metadata_0_17_1 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.18.0.scale")]
pub mod midnight_metadata_0_18_0 {}

pub use midnight_metadata_0_18_0 as midnight_metadata_latest;
