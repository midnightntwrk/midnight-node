use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct Manifest {
	pub package: Package,
}

#[derive(Deserialize)]
pub(crate) struct Package {
	pub version: String,
}

#[macro_export]
macro_rules! find_crate_version {
	($cargo_toml_path:literal) => {{
		let manifest_str = include_str!($cargo_toml_path);
		let manifest: crate::utils::Manifest =
			toml::from_str(&manifest_str).expect("Failed to parse manifest");

		manifest.package.version
	}};
}

pub(crate) use find_crate_version;
