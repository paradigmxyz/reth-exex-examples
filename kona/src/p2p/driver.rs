//! Driver for the P2P Service.

use anyhow::Result;
use std::net::SocketAddr;
use alloy::primitives::Address;
use libp2p_identity::Keypair;
use tokio::select;
use tokio::sync::watch::{Receiver, Sender};
use libp2p::{Multiaddr, SwarmBuilder, PeerId, Transport};
use libp2p::swarm::SwarmEvent;

use crate::p2p::event::Event;
use crate::p2p::behaviour::Behaviour;
use crate::p2p::types::ExecutionPayloadEnvelope;

/// Driver contains the logic for the P2P service.
pub struct GossipDriver {
    /// The [Behaviour] of the node.
    pub behaviour: Behaviour,
    /// Channel to receive unsafe blocks.
    pub unsafe_block_recv: Receiver<ExecutionPayloadEnvelope>,
    /// Channel to send unsafe signer updates.
    pub unsafe_block_signer_sender: Sender<Address>,
    /// The socket address that the service is listening on.
    pub addr: SocketAddr,
    /// The chain ID of the network.
    pub chain_id: u64,
    /// A unique keypair to validate the node's identity
    pub keypair: Keypair,
}

impl GossipDriver {
    /// Starts the Discv5 peer discovery & libp2p services
    /// and continually listens for new peers and messages to handle
    pub fn start(mut self) -> Result<()> {

        // TODO: pull this swarm building out into the builder
        let addr = NetworkAddress::try_from(self.addr)?;
        let tcp_cfg = libp2p::tcp::Config::default();
        let auth_cfg = libp2p::noise::Config::new(&self.keypair)?;
        let transport = libp2p::tcp::tokio::Transport::new(tcp_cfg)
            .upgrade(libp2p::core::upgrade::Version::V1Lazy)
            .authenticate(auth_cfg)
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
                        if let SwarmEvent::Behaviour(Event::Gossipsub(libp2p::gossipsub::Event::Message {
                            propagation_source: peer_id,
                            message_id: id,
                            message,
                        })) = event {
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