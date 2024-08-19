//! Block Handler

use eyre::Result;
use libp2p::swarm::NetworkBehaviour;
use libp2p::gossipsub::Config;
use libp2p::gossipsub::MessageAuthenticity;
use libp2p::gossipsub::IdentTopic;

use crate::p2p::event::Event;
use crate::p2p::handler::Handler;

/// Specifies the [NetworkBehaviour] of the node
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
pub struct Behaviour {
    /// Responds to inbound pings and send outbound pings.
    ping: libp2p::ping::Behaviour,
    /// Enables gossipsub as the routing layer.
    gossipsub: libp2p::gossipsub::Behaviour,
}

impl Behaviour {
    /// Configures the swarm behaviors, subscribes to the gossip topics, and returns a new [Behaviour].
    fn new(cfg: Config, handlers: &[Box<dyn Handler>]) -> Result<Self> {
        let ping = libp2p::ping::Behaviour::default();

        let mut gossipsub =
            libp2p::gossipsub::Behaviour::new(MessageAuthenticity::Anonymous, cfg)
                .map_err(|_| eyre::eyre!("gossipsub behaviour creation failed"))?;

        handlers
            .iter()
            .flat_map(|handler| {
                handler
                    .topics()
                    .iter()
                    .map(|topic| {
                        let topic = IdentTopic::new(topic.to_string());
                        gossipsub
                            .subscribe(&topic)
                            .map_err(|_| eyre::eyre!("subscription failed"))
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Result<Vec<bool>>>()?;

        Ok(Self { ping, gossipsub })
    }
}
