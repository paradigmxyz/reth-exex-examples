# Reth Execution Extension (ExEx) Examples

This repository is a collection of [ExEx](https://reth.rs/developers/exex/exex.html) examples
built on [Reth](https://github.com/paradigmxyz/reth) that demonstrates common patterns and may serve as an inspiration
for new developers.

[![Telegram chat][telegram-badge]][telegram-url]

[telegram-badge]: https://img.shields.io/endpoint?color=neon&style=for-the-badge&url=https%3A%2F%2Ftg.sumanjay.workers.dev%2Fparadigm_reth
[telegram-url]: https://t.me/paradigm_reth

## Overview

| Example                              | Description                                                                | Run                                                                                                                                                                                                                                                           |
| ------------------------------------ | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Backfill](./backfill)               | Exposes an RPC to backfill (execute) a range of blocks                     | `cargo run --bin backfill -- node --http` to start Reth node with an RPC endpoint opened on HTTP port 8545<br><br>`ETH_RPC_URL=http://localhost:8545 TIP=$(cast bn); cast rpc backfill_start $(($TIP - 10)) $TIP` to start the backfill of the last 10 blocks |
| [Discv5](./discv5)                   | Runs discv5 discovery stack                                                | `cargo run --bin discv5`                                                                                                                                                                                                                                      |
| [In Memory State](./in-memory-state) | Tracks the plain state in memory                                           | `cargo run --bin in-memory-state -- node`                                                                                                                                                                                                                     |
| [Minimal](./minimal)                 | Logs every chain commit, reorg and revert notification                     | `cargo run --bin minimal -- node`                                                                                                                                                                                                                             |
| [OP Bridge](./op-bridge)             | Decodes Optimism deposit and withdrawal receipts from L1                   | `cargo run --bin op-bridge -- node`                                                                                                                                                                                                                           |
| [Oracle](./oracle)                   | Oracle protocol that observes off-chain data and attests to it on-chain    | `cargo run --bin oracle -- node`                                                                                                                                                                                                                              |
| [Remote](./remote)                   | Emits notifications using a gRPC server, and a consumer that receives them | `cargo run --bin remote-exex -- node` to start Reth node with the ExEx and a gRPC server<br><br>`cargo run --bin remote-consumer` to start a gRPC client                                                                                                      |
| [Rollup](./rollup)                   | Rollup that derives the state from L1                                      | `cargo run --bin rollup -- node`                                                                                                                                                                                                                              |

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in these crates by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
</sub>
