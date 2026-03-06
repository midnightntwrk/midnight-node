// Generate an interface that we can use from the node's metadata.
#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.17.0.scale")]
pub mod midnight_metadata_0_17_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.17.1.scale")]
pub mod midnight_metadata_0_17_1 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.18.0.scale")]
pub mod midnight_metadata_0_18_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.18.1.scale")]
pub mod midnight_metadata_0_18_1 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.19.0.scale")]
pub mod midnight_metadata_0_19_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.20.0.scale")]
pub mod midnight_metadata_0_20_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.20.1.scale")]
pub mod midnight_metadata_0_20_1 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.21.0.scale")]
pub mod midnight_metadata_0_21_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.22.0.scale")]
pub mod midnight_metadata_0_22_0 {}

pub use midnight_metadata_0_22_0 as midnight_metadata_latest;
