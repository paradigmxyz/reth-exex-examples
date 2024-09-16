use reth_tracing::tracing::error;
use tokio::sync::{
    broadcast::{error::RecvError, Sender},
    mpsc::UnboundedSender,
};

use super::proto::{connection::OracleCommand, data::SignedTicker};

/// The size of the broadcast channel.
///
/// This value is based on the estimated message rate and the tolerance for lag.
/// - We assume an average of 10-20 updates per second per symbol.
/// - For 2 symbols (e.g., ETHUSDC and BTCUSDC), this gives approximately 20-40 messages per second.
/// - To allow subscribers to catch up if they fall behind, we provide a lag tolerance of 5 seconds.
///
/// Thus, the buffer size is calculated as:
///
/// `Buffer Size = Message Rate per Second * Lag Tolerance`
///
/// For 2 symbols, we calculate: `40 * 5 = 200`.
const BROADCAST_CHANNEL_SIZE: usize = 200;

pub(crate) struct Gossip {
    pub(crate) inner: Sender<SignedTicker>,
}

impl Gossip {
    /// Creates a new Broadcast instance.
    pub(crate) fn new() -> (Self, tokio::sync::broadcast::Sender<SignedTicker>) {
        let (sender, _receiver) = tokio::sync::broadcast::channel(BROADCAST_CHANNEL_SIZE);
        (Self { inner: sender.clone() }, sender)
    }

    /// Starts to gossip data to the connected peers from the broadcast channel.
    pub(crate) fn start(
        &self,
        to_connection: UnboundedSender<OracleCommand>,
    ) -> Result<(), RecvError> {
        let mut receiver = self.inner.subscribe();

        tokio::task::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(signed_data) => {
                        if let Err(e) = to_connection.send(OracleCommand::Tick(signed_data)) {
                            error!(?e, "Failed to broadcast message to peer");
                        }
                    }
                    Err(e) => {
                        error!(?e, "Data feed task encountered an error");
                        return Err::<(), RecvError>(e);
                    }
                }
            }
        });

        Ok(())
    }
}
