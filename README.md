# Reth Execution Extension (ExEx) Examples

This repository is a collection of [ExEx](https://reth.rs/developers/exex/exex.html) examples
built on [Reth](https://github.com/paradigmxyz/reth) that demonstrates common patterns and may serve as an inspiration
for new developers.

[![Telegram chat][telegram-badge]][telegram-url]

[telegram-badge]: https://img.shields.io/endpoint?color=neon&style=for-the-badge&url=https%3A%2F%2Ftg.sumanjay.workers.dev%2Fparadigm_reth
[telegram-url]: https://t.me/paradigm_reth

## Overview

| Example              | Description                                                                | Run                                                                                                                                                  |
| -------------------- | -------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`Remote`](./remote) | Emits notifications using a gRPC server, and a consumer that receives them | `cargo run --bin remote-exex -- node` to start Reth node with the ExEx and a gRPC server<br>`cargo run --bin remote-consumer` to start a gRPC client |

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
