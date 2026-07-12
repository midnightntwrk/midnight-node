#[test]
fn cli_tests() {
	// Run from the crate dir: cargo runs tests there, but buck2 runs from the
	// workspace root — where `README.md` is a different file (with an ASCII-art
	// box), `tests/cmd/` doesn't exist, and the tomls' `../../res/...` fixture
	// paths don't resolve. CARGO_MANIFEST_DIR is the crate dir under both (buck
	// sets it to "util/toolkit"), so chdir there makes every relative path agree.
	// Safe: cli_tests is the only test in this integration-test binary.

	// Absolutize the toolkit bin path BEFORE chdir: buck sets CARGO_BIN_EXE_* to
	// a path relative to the workspace root (the current CWD); after chdir it
	// wouldn't resolve. cargo already sets an absolute path (join is a no-op).
	let bin = std::env::var_os("CARGO_BIN_EXE_midnight-node-toolkit")
		.map(|p| std::env::current_dir().unwrap().join(p));

	std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).unwrap();
	// Create directory to put test outputs in
	std::fs::create_dir_all("out").unwrap();

	let cases = trycmd::TestCases::new();
	if let Some(bin) = bin {
		cases.register_bin("midnight-node-toolkit", bin);
	}
	cases.case("tests/cmd/*.toml").case("README.md");
}
