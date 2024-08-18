//! L2 Consensus P2P Stack
//!
//! ## Building a [`driver::GossipDriver`]
//!
//! ```rust
//! use std::net::SocketAddr;
//! use alloy_primitives::address;
//!
//! // Builder inputs
//! let chain_id: u64 = 10; // OP Mainnet
//! let unsafe_signer = address!("babe");
//! let socket: SocketAddr = "0.0.0.0:9845".parse().unwrap():
//!
//! let driver = GossipBuilder::new()
//!     .with_chain_id(chain_id)
//!     .with_unsafe_block_signer(unsafe_signer)
//!     .with_socket(socket)
//!     .with_blocks_topic_v1()
//!     .with_blocks_topic_v2()
//!     .with_blocks_topic_v3()
//!     .build()?;
//! ```

pub mod builder;
pub mod config;
pub mod topics;
pub mod gossip;
pub mod driver;
pub mod behaviour;
pub mod event;
pub mod handler;
