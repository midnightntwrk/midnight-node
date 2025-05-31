fn main() {
	println!("cargo::rustc-check-cfg=cfg(hardfork_test)");
	println!("cargo:re-run-if-env-changed=HARDFORK_TEST");
	if std::env::var("HARDFORK_TEST").is_ok() {
		println!("cargo:rustc-cfg=hardfork_test");
	}
}
