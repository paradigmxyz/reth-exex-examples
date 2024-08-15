### Run reth (/)

```console
export ETHERSCAN_API_KEY={ETHERSCAN_API_KEY} && cargo run -- node --debug.etherscan --chain holesky --http 
```

### Build wasm (/wasm_exex)

- download https://github.com/bytecodealliance/cargo-wasi

```console
cargo install cargo-wasi
```

- build wasi

```console
cargo wasi build --release
```

### Send wasm

```console
cargo wasm_install
```

```console
cargo wasm_send
```
