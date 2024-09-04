use eyre::Result;
use futures::Future;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use reth_tracing::tracing::info;
use std::{
    pin::Pin,
    task::{ready, Context, Poll},
};

/// The ExEx struct, representing the initialization and execution of the ExEx.
pub struct ExEx<Node: FullNodeComponents> {
    exex: ExExContext<Node>,
}

impl<Node: FullNodeComponents> ExEx<Node> {
    pub fn new(exex: ExExContext<Node>) -> Self {
        Self { exex }
    }
}

impl<Node: FullNodeComponents> Future for ExEx<Node> {
    type Output = Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Continuously poll the ExExContext notifications
        loop {
            if let Some(notification) = ready!(self.exex.notifications.poll_recv(cx)) {
                match &notification {
                    ExExNotification::ChainCommitted { new } => {
                        info!(committed_chain = ?new.range(), "Received commit");
                    }
                    ExExNotification::ChainReorged { old, new } => {
                        info!(from_chain = ?old.range(), to_chain = ?new.range(), "Received reorg");
                    }
                    ExExNotification::ChainReverted { old } => {
                        info!(reverted_chain = ?old.range(), "Received revert");
                    }
                }

                if let Some(committed_chain) = notification.committed_chain() {
                    self.exex
                        .events
                        .send(ExExEvent::FinishedHeight(committed_chain.tip().number))?;
                }
            }
        }
    }
}
