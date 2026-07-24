// Generate an interface that we can use from the node's metadata.
#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.21.0.scale")]
pub mod midnight_metadata_0_21_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.22.0.scale")]
pub mod midnight_metadata_0_22_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_1.0.0.scale")]
pub mod midnight_metadata_1_0_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_2.0.0.scale")]
pub mod midnight_metadata_2_0_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_2.1.0.scale")]
pub mod midnight_metadata_2_1_0 {}

pub use midnight_metadata_2_1_0 as midnight_metadata_latest;
