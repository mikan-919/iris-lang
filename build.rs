use std::env;
use std::process::Command;

fn main() {
    let profile = env::var("PROFILE").unwrap_or_default();

    let wasm_path = format!("target/wasm32-unknown-unknown/{}/iris_lang.wasm", profile);

    // Only run wasm-bindgen if the wasm file exists
    if std::path::Path::new(&wasm_path).exists() {
        let status = Command::new("wasm-bindgen")
            .args(&["--out-dir", "iris-web/pkg/", "--target", "web", &wasm_path])
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
