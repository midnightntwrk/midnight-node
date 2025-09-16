mod config;
use config::load_config;
use std::fs;
use std::path::Path;

#[test]
fn print_cnight_policy_file() {
    let cfg = load_config();
    let path = Path::new(&cfg.cnight_policy_file);
    let file_content = fs::read_to_string(path)
        .expect("Failed to read cnight_policy_file");
    // Try to parse as JSON and extract cborHex
    let cbor_hex = match serde_json::from_str::<serde_json::Value>(&file_content) {
        Ok(json) => json.get("cborHex")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        Err(_) => None,
    };
    if let Some(hex) = cbor_hex {
        println!("{}", hex);
    } else {
        // If not JSON, print as hex
        let raw_bytes = fs::read(path).expect("Failed to read cnight_policy_file as bytes");
        println!("{}", hex::encode(raw_bytes));
    }
}
