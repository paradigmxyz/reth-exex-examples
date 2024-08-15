use base64::prelude::*;
use reqwest::Client;
use serde_json::json;
use std::fs::File;
use std::io::Read;

pub async fn install_wasm(url: &str, name: &str, wasm_file_path: &str) -> eyre::Result<()> {
    let mut file = File::open(wasm_file_path)?;
    let mut wasm_bytes = Vec::new();
    file.read_to_end(&mut wasm_bytes)?;

    let wasm_base64 = BASE64_STANDARD.encode(&wasm_bytes);

    let payload = json!({
        "jsonrpc": "2.0",
        "method": "exex_install",
        "params": {
            "name": name,
            "wasm_base64": wasm_base64
        },
        "id": 1
    });

    let client = Client::new();
    let res = client.post(url).json(&payload).send().await?;
    let res = res.json::<serde_json::Value>().await?;

    println!("{}", serde_json::to_string_pretty(&res)?);

    Ok(())
}

pub async fn start_wasm(url: &str, name: &str) -> eyre::Result<()> {
    let payload = json!({
        "jsonrpc": "2.0",
        "method": "exex_start",
        "params": {
            "name": name
        },
        "id": 1
    });

    let client = Client::new();
    let res = client.post(url).json(&payload).send().await?;
    let res = res.json::<serde_json::Value>().await?;

    println!("{}", serde_json::to_string_pretty(&res)?);

    Ok(())
}
