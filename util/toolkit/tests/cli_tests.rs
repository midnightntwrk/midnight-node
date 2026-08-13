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

	// Under cargo, chdir to the crate dir (CARGO_MANIFEST_DIR) — where tests/cmd/,
	// README.md, and the tomls' `../../res/...` all resolve. Under buck2 the RE test
	// sandbox has none of that; MN_CLI_FIXTURES_ROOT points at a staged mirror that
	// reproduces the same 2-levels-deep layout (util/toolkit/{tests/cmd,README.md}
	// with res/ two levels up) so the unchanged `../../res/...` paths still resolve.
	// compile-time env!() would bake buck2's dead build-sandbox path, so read at runtime.
	let cwd = std::env::var("MN_CLI_FIXTURES_ROOT")
		.or_else(|_| std::env::var("CARGO_MANIFEST_DIR"))
		.unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string());
	std::env::set_current_dir(&cwd).unwrap();
	// Create directory to put test outputs in
	std::fs::create_dir_all("out").unwrap();

	let cases = trycmd::TestCases::new();
	if let Some(bin) = bin {
		cases.register_bin("midnight-node-toolkit", bin);
	}
	cases.case("tests/cmd/*.toml").case("README.md");
}
