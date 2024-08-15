use base64::prelude::*;
use eyre::ensure;
use reqwest::Client;
use serde_json::json;
use std::fs::File;
use std::io::Read;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let subcommand = &args[1];
    let url = &args[2];

    match subcommand.as_str() {
        "install" => {
            ensure!(args.len() >= 3, "Usage: install <JSON-RPC URL> <name> <WASM file path>");
            let wasm_file_path = &args[3];
            let name = &args[4];
            install(url, wasm_file_path, name).await?
        }

        "start" => {
            ensure!(args.len() >= 3, "Usage: install <JSON-RPC URL> <name>");
            let name = &args[3];
            start(url, name).await?
        }
        _ => println!("Invalid subcommand. Use 'install' or 'start'."),
    }

    Ok(())
}

async fn install(url: &str, name: &str, wasm_file_path: &str) -> eyre::Result<()> {
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

    println!("Install Response: {:?}", res.text().await?);

    Ok(())
}

async fn start(url: &str, name: &str) -> eyre::Result<()> {
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

    println!("Start Response: {:?}", res.text().await?);

    Ok(())
}
