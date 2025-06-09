use walkdir::WalkDir;

fn main() {
	// Track all files in the res crate's root
	for entry in WalkDir::new(".")
		.into_iter()
		.filter_map(|e| e.ok())
		.filter(|e| e.file_type().is_file())
	{
		println!("cargo:rerun-if-changed={}", entry.path().display());
	}
}
