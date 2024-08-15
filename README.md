### Run reth (/)

```console
export ETHERSCAN_API_KEY={ETHERSCAN_API_KEY} && cargo run -- node --debug.etherscan --chain holesky --http 
```

### Build wasm (/wasm_exex)

```console
cargo build -r --target wasm32-unknown-unknown
```

### Send wasm

```console
cargo wasm_install
```

```console
cargo wasm_send
```
