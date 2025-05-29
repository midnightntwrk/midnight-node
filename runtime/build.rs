fn main() {
	println!("cargo::rustc-check-cfg=cfg(hardfork_test)");
	println!("cargo:re-run-if-env-changed=HARDFORK_TEST");
	if std::env::var("HARDFORK_TEST").is_ok() {
		println!("cargo:rustc-cfg=hardfork_test");
		unsafe {
			std::env::set_var("FORCE_WASM_BUILD", "true");
		}
	}

	println!("cargo::rustc-check-cfg=cfg(hardfork_test_rollback)");
	println!("cargo:re-run-if-env-changed=HARDFORK_TEST_ROLLBACK");
	if std::env::var("HARDFORK_TEST_ROLLBACK").is_ok() {
		println!("cargo:rustc-cfg=hardfork_test_rollback");
		unsafe {
			std::env::set_var("FORCE_WASM_BUILD", "true");
		}
	}

	#[cfg(feature = "std")]
	{
		substrate_wasm_builder::WasmBuilder::new()
			.with_current_project()
			.export_heap_base()
			.import_memory()
			.build();
	}
}
