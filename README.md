### Run reth (/)

```console
export ETHERSCAN_API_KEY={ETHERSCAN_API_KEY} && cargo run -- node --debug.etherscan --chain holesky --http 
```

### Build wasm (/wasm_exex)

```console
cargo build -r --target wasm32-unknown-unknown
```

### Send wasm (/wasm_send)

```console
cargo run -- http://127.0.0.1:8545 ../target/wasm32-unknown-unknown/release/wasm-exex.wasm
```
