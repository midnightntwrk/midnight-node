#[test]
fn cli_tests() {
	// Create directory to put test outputs in
	std::fs::create_dir_all("out").unwrap();
	trycmd::TestCases::new().case("tests/cmd/*.toml").case("README.md");
}
