use super::{OracleProtoMessage, OracleProtoMessageKind, ProtocolEvent, ProtocolState};
use futures::{Stream, StreamExt};
use reth_eth_wire::{
    capability::SharedCapabilities, multiplex::ProtocolConnection, protocol::Protocol,
};
use reth_network::protocol::{ConnectionHandler, OnNotSupported};
use reth_network_api::Direction;
use reth_primitives::BytesMut;
use reth_rpc_types::PeerId;
use std::{
    pin::Pin,
    task::{ready, Context, Poll},
};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::UnboundedReceiverStream;

/// The commands supported by the OracleConnection.
pub(crate) enum OracleCommand {
    /// Sends a message to the peer
    Message {
        msg: String,
        /// The response will be sent to this channel.
        response: oneshot::Sender<String>,
    },
}

/// This struct defines the connection object for the Oracle subprotocol.
pub(crate) struct OracleConnection {
    conn: ProtocolConnection,
    commands: UnboundedReceiverStream<OracleCommand>,
    pending_pong: Option<oneshot::Sender<String>>,
    initial_ping: Option<OracleProtoMessage>,
}

impl Stream for OracleConnection {
    type Item = BytesMut;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if let Some(initial_ping) = this.initial_ping.take() {
            return Poll::Ready(Some(initial_ping.encoded()));
        }

        let Some(msg) = ready!(this.conn.poll_next_unpin(cx)) else { return Poll::Ready(None) };

        let Some(msg) = OracleProtoMessage::decode_message(&mut &msg[..]) else {
            return Poll::Ready(None);
        };

        match msg.message {
            OracleProtoMessageKind::Ping => {
                return Poll::Ready(Some(OracleProtoMessage::pong().encoded()))
            }
            OracleProtoMessageKind::Pong => {}
            OracleProtoMessageKind::SignedTicker(_) => {
                // TODO: verify signature and keep count
            }
        }

        Poll::Pending
    }
}

/// The connection handler for the RLPx subprotocol.
pub(crate) struct OracleConnHandler {
    pub(crate) state: ProtocolState,
}

impl ConnectionHandler for OracleConnHandler {
    type Connection = OracleConnection;

    fn protocol(&self) -> Protocol {
        OracleProtoMessage::protocol()
    }

    fn on_unsupported_by_peer(
        self,
        _supported: &SharedCapabilities,
        _direction: Direction,
        _peer_id: PeerId,
    ) -> OnNotSupported {
        OnNotSupported::KeepAlive
    }

    fn into_connection(
        self,
        direction: Direction,
        peer_id: PeerId,
        conn: ProtocolConnection,
    ) -> Self::Connection {
        let (tx, rx) = mpsc::unbounded_channel();
        self.state
            .events
            .send(ProtocolEvent::Established { direction, peer_id, to_connection: tx })
            .ok();
        OracleConnection {
            conn,
            initial_ping: direction.is_outgoing().then(OracleProtoMessage::ping),
            commands: UnboundedReceiverStream::new(rx),
            pending_pong: None,
        }
    }
}
