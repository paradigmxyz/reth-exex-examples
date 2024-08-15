use base64::prelude::*;
use eyre::ensure;
use reqwest::Client;
use serde_json::json;
use std::fs::File;
use std::io::Read;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    // Get the JSON-RPC URL endpoint and WASM file path from command line arguments
    let args: Vec<String> = std::env::args().collect();
    ensure!(args.len() >= 3, "<JSON-RPC URL> <WASM file path>");

    let url = &args[1];
    let wasm_file_path = &args[2];

    // Read the WASM binary from the file specified in command line arguments
    let mut file = File::open(wasm_file_path)?;
    let mut wasm_bytes = Vec::new();
    file.read_to_end(&mut wasm_bytes)?;

    // Serialize the WASM bytes into a base64 string
    let wasm_base64 = BASE64_STANDARD.encode(&wasm_bytes);

    // Create the JSON-RPC payload
    let payload = json!({
        "jsonrpc": "2.0",
        "method": "exex_install",
        "params": {
            "name": "my_wasm_module",
            "bytecode": wasm_base64
        },
        "id": 1
    });

    // Send the payload to the JSON-RPC endpoint provided as an argument
    let client = Client::new();
    let res = client.post(url).json(&payload).send().await?;

    println!("Response: {:?}", res.text().await?);

    Ok(())
}
