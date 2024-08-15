### Run reth

```console
export ETHERSCAN_API_KEY={ETHERSCAN_API_KEY} && cargo run -- node --debug.etherscan --chain holesky --http 
```

### Send wasm

```console
cargo run -- http://127.0.0.1:8545 ../wasm_exex/target/wasm32-unknown-unknown/debug/wasm-exex.wasm
```
