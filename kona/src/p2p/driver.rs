//! Driver for the P2P Service.

use anyhow::Result;
use std::net::SocketAddr;
use alloy_primitives::Address;
use libp2p_identity::Keypair;
use tokio::select;
use tokio::sync::watch::{channel, Receiver, Sender};
use kona_derive::types::ExecutionPayload;

use crate::p2p::behaviour::Behaviour;

use libp2p::{
    gossipsub::{self, IdentTopic, Message, MessageId},
    mplex::MplexConfig,
    noise, ping,
    swarm::{NetworkBehaviour, SwarmBuilder, SwarmEvent},
    tcp, Multiaddr, PeerId, Swarm, Transport,
};

/// Driver contains the logic for the P2P service.
#[derive(Debug)]
pub struct GossipDriver {
    /// The [Behaviour] of the node.
    behaviour: Behaviour,
    /// Channel to receive unsafe blocks.
    unsafe_block_recv: Receiver<ExecutionPayload>,
    /// Channel to send unsafe signer updates.
    unsafe_block_signer_sender: Sender<Address>,
    /// The socket address that the service is listening on.
    addr: SocketAddr,
    /// The chain ID of the network.
    chain_id: u64,
    /// A unique keypair to validate the node's identity
    keypair: Keypair,
}

impl GossipDriver {
    /// Instantiates a new [GossipDriver].
    pub fn new(chain_id: u64, unsafe_block_signer: Address, socket: SocketAddr) -> Self {
        Self {
            unsafe_block_recv,
            unsafe_block_signer_sender,
            addr: socket,
            chain_id,
            keypair: Keypair::generate_secp256k1(),
        }
    }

    /// Starts the Discv5 peer discovery & libp2p services
    /// and continually listens for new peers and messages to handle
    pub fn start(mut self) -> Result<()> {

        // TODO: pull this swarm building out into the builder
        let addr = NetworkAddress::try_from(self.addr)?;
        let tcp_cfg = libp2p::tcp::Config::default();
        let auth_cfg = libp2p::noise::Config::new(&self.keypair)?;
        let mplex_cfg = libp2p::mplex::MplexConfig::default();
        let transport = libp2p::tcp::tokio::Transport::new(tcp_cfg)
            .upgrade(libp2p::core::upgrade::Version::V1Lazy)
            .authenticate(auth_cfg)
            .multiplex(mplex_cfg)
            .boxed();
        let mut swarm = SwarmBuilder::with_tokio_executor(transport, self.behaviour, PeerId::from(self.keypair.public()))
            .build();
        let mut peer_recv = discovery::start(addr, self.chain_id)?;
        let multiaddr = Multiaddr::from(addr);
        swarm
            .listen_on(multiaddr)
            .map_err(|_| eyre::eyre!("swarm listen failed"))?;

        let mut handlers = Vec::new();
        handlers.append(&mut self.handlers);

        tokio::spawn(async move {
            loop {
                select! {
                    peer = peer_recv.recv().fuse() => {
                        if let Some(peer) = peer {
                            let peer = Multiaddr::from(peer);
                            _ = swarm.dial(peer);
                        }
                    },
                    event = swarm.select_next_some() => {
                        if let SwarmEvent::Behaviour(Event::Gossipsub(gossipsub::Event::Message {
                            propagation_source: peer_id,
                            message_id: id,
                            message,
                        })) => {
                            let handler = self.handlers
                                .iter()
                                .find(|h| h.topics().contains(&message.topic));
                            if let Some(handler) = handler {
                                let status = handler.handle(message);
                                _ = swarm
                                    .behaviour_mut()
                                    .gossipsub
                                    .report_message_validation_result(&message_id, &propagation_source, status);
                            }
                        }
                    },
                }
            }
        });

        Ok(())
    }
}
