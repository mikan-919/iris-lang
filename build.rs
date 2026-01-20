use std::env;
use std::process::Command;

fn main() {
    let profile = env::var("PROFILE").unwrap_or_default();

    let wasm_path = format!("target/wasm32-unknown-unknown/{}/iris_lang.wasm", profile);

    // Only run wasm-bindgen if wasm file exists
    if std::path::Path::new(&wasm_path).exists() {
        let args: Vec<String> = vec![
            "--out-dir".to_string(),
            "iris-web/pkg/".to_string(),
            "--target".to_string(),
            "web".to_string(),
            wasm_path,
        ];
        let status = Command::new("wasm-bindgen")
            .args(args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .status()
            .expect("Failed to run wasm-bindgen");

        if !status.success() {
            panic!("wasm-bindgen failed");
        }

        println!("Generated wasm bindings in iris-web/pkg/");
    } else {
        println!(
            "Wasm file not found at {}, skipping wasm-bindgen",
            wasm_path
        );
    }
}
