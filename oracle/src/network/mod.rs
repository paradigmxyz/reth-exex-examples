use futures::FutureExt;
use reth_tracing::tracing::{error, info};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

mod discovery;

/// The Network struct is a long running task that orchestrates discovery of new peers and network
/// gossiping via the RLPx subprotocol.
pub(crate) struct Network {
    /// The discovery task for this node.
    discovery: discovery::Discovery,
}

impl Network {
    pub(crate) async fn new(tcp_port: u16, udp_port: u16) -> eyre::Result<Self> {
        let discovery = discovery::Discovery::new(tcp_port, udp_port).await?;
        Ok(Self { discovery })
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
                    info!("Discv5 task completed successfully");
                }
                Poll::Ready(Err(e)) => {
                    error!(?e, "Discv5 task encountered an error");
                    return Poll::Ready(Err(e));
                }
                Poll::Pending => {}
            }
        }
    }
}
