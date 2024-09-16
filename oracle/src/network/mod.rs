use discovery::Discovery;
use futures::FutureExt;
use gossip::Gossip;
use proto::{data::SignedTicker, ProtocolEvent};
use reth_tracing::tracing::{error, info};
use std::{
    future::Future,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::broadcast::Sender;

mod discovery;
mod gossip;
pub(crate) mod proto;

/// The Network struct is a long running task that orchestrates discovery of new peers and network
/// gossiping via the RLPx subprotocol.
pub(crate) struct Network {
    /// The discovery task for this node.
    discovery: Discovery,
    /// The protocol events channel.
    proto_events: proto::ProtoEvents,
    /// Helper struct to manage gossiping data to connected peers.
    gossip: Gossip,
}

impl Network {
    pub(crate) async fn new(
        proto_events: proto::ProtoEvents,
        tcp_port: u16,
        udp_port: u16,
    ) -> eyre::Result<(Self, Sender<SignedTicker>)> {
        let disc_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), udp_port);
        let rlpx_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), tcp_port);
        let discovery = Discovery::new(disc_addr, rlpx_addr).await?;
        let (gossip, to_gossip) = Gossip::new();
        Ok((Self { discovery, proto_events, gossip }, to_gossip))
    }
}

impl Future for Network {
    type Output = eyre::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.as_mut();
        // Poll the discovery future until its drained
        loop {
            match this.discovery.poll_unpin(cx) {
                Poll::Ready(Ok(())) => {
                    info!("Discovery task completed");
                }
                Poll::Ready(Err(e)) => {
                    error!(?e, "Discovery task encountered an error");
                    return Poll::Ready(Err(e));
                }
                Poll::Pending => break,
            }
        }

        loop {
            match this.proto_events.poll_recv(cx) {
                Poll::Ready(Some(ProtocolEvent::Established {
                    direction,
                    peer_id,
                    to_connection,
                })) => {
                    info!(
                        ?direction,
                        ?peer_id,
                        ?to_connection,
                        "Established connection, will start gossiping"
                    );
                    this.gossip.start(to_connection)?;
                }
                Poll::Ready(None) => {
                    return Poll::Ready(Ok(()));
                }

                Poll::Pending => {}
            }
        }
    }
}
