use std::fs::File;
use std::io::Read;
use serde_json::json;
use reqwest::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read the WASM binary from a file
    let mut file = File::open("path/to/your/wasm/file.wasm")?;
    let mut wasm_bytes = Vec::new();
    file.read_to_end(&mut wasm_bytes)?;

    // Serialize the WASM bytes into a base64 string
    let wasm_base64 = base64::encode(&wasm_bytes);

    // Create the JSON-RPC payload
    let payload = json!({
        "jsonrpc": "2.0",
        "method": "deploy_wasm",
        "params": {
            "wasm_binary": wasm_base64
        },
        "id": 1
    });

    // Send the payload to a JSON-RPC endpoint
    let client = Client::new();
    let res = client.post("http://your-json-rpc-endpoint")
        .json(&payload)
        .send()
        .await?;

    println!("Response: {:?}", res.text().await?);

    Ok(())
}
