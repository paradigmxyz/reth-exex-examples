//! Builder for an OP Stack P2P network.

use anyhow::Result;
use std::net::SocketAddr;
use alloy::primitives::Address;
use libp2p_identity::Keypair;
use tokio::sync::watch::channel;

use crate::p2p::config;
use crate::p2p::handler::BlockHandler;
use crate::p2p::config::ConfigBuilder;
use crate::p2p::behaviour::Behaviour;
use crate::p2p::driver::GossipDriver;

/// Constructs a [GossipDriver] for the OP Stack P2P network.
#[derive(Debug, Clone, Default)]
pub struct GossipDriverBuilder {
    /// The chain ID of the network.
    chain_id: Option<u64>,
    /// The unsafe block signer.
    unsafe_block_signer: Option<Address>,
    /// The socket address that the service is listening on.
    socket: Option<SocketAddr>,
    /// The [ConfigBuilder] constructs the [Config] for `gossipsub`.
    inner: Option<ConfigBuilder>,
    /// The [Keypair] for the node.
    keypair: Option<Keypair>,
}

impl GossipDriverBuilder {
    /// Creates a new [GossipDriverBuilder].
    pub fn new() -> Self {
        Self::default()
    }

    /// Specifies the chain ID of the network.
    pub fn with_chain_id(&mut self, chain_id: u64) -> &mut Self {
        self.chain_id = Some(chain_id);
        self
    }

    /// Specifies the unsafe block signer.
    pub fn with_unsafe_block_signer(&mut self, unsafe_block_signer: Address) -> &mut Self {
        self.unsafe_block_signer = Some(unsafe_block_signer);
        self
    }

    /// Specifies the socket address that the service is listening on.
    pub fn with_socket(&mut self, socket: SocketAddr) -> &mut Self {
        self.socket = Some(socket);
        self
    }

    /// Specifies the keypair for the node.
    pub fn with_keypair(&mut self, keypair: Keypair) -> &mut Self {
        self.keypair = Some(keypair);
        self
    }

    // TODO: extend builder with [ConfigBuilder] methods.

    /// Specifies the [ConfigBuilder] for the `gossipsub` configuration.
    pub fn with_gossip_config(&mut self, inner: ConfigBuilder) -> &mut Self {
        self.inner = Some(inner);
        self
    }

    /// Builds the [GossipDriver].
    pub fn build(self) -> Result<GossipDriver> {
        // Build the config for gossipsub.
        let config = self.inner.unwrap_or(config::default_config_builder()).build()?;

        // Create the block handler.
        let (unsafe_block_signer_sender, unsafe_block_signer_recv) = channel(unsafe_block_signer);
        let (block_handler, unsafe_block_recv) = BlockHandler::new(&self.chain_id, unsafe_block_signer_recv);

        // Construct the gossipsub behaviour.
        let behaviour = Behaviour::new(config, &block_handler)?;

        Ok(GossipDriver {
            behaviour,
            unsafe_block_recv,
            unsafe_block_signer_sender,
            chain_id: self.chain_id.ok_or_else(|| eyre::eyre!("chain ID not set"))?,
            addr: self.socket.ok_or_else(|| eyre::eyre!("socket address not set"))?,
            keypair: self.keypair.unwrap_or(Keypair::generate_secp256k1()),
        })
    }
}
