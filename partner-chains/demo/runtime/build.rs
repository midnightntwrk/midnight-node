fn main() {
	#[cfg(all(feature = "std", feature = "wasm-runtime"))]
	{
		substrate_wasm_builder::WasmBuilder::new()
			.with_current_project()
			.export_heap_base()
			.import_memory()
			.build();
	}
}
