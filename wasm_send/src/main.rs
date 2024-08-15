use eyre::ensure;
use wasm_send::{install_wasm, start_wasm};

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
            install_wasm(url, wasm_file_path, name).await?
        }

        "start" => {
            ensure!(args.len() >= 3, "Usage: install <JSON-RPC URL> <name>");
            let name = &args[3];
            start_wasm(url, name).await?
        }

        _ => println!("Invalid subcommand. Use 'install' or 'start'."),
    }

    Ok(())
}
