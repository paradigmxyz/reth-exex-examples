use super::{
    data::SignedTicker, OracleProtoMessage, OracleProtoMessageKind, ProtocolEvent, ProtocolState,
};
use alloy_rlp::Encodable;
use futures::{Stream, StreamExt};
use reth_eth_wire::{
    capability::SharedCapabilities, multiplex::ProtocolConnection, protocol::Protocol,
};
use reth_network::protocol::{ConnectionHandler, OnNotSupported};
use reth_network_api::Direction;
use reth_primitives::{Address, BytesMut, B256};
use reth_rpc_types::PeerId;
use std::{
    collections::HashMap,
    pin::Pin,
    task::{ready, Context, Poll},
};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::{BroadcastStream, UnboundedReceiverStream};

/// The commands supported by the OracleConnection.
pub(crate) enum OracleCommand {
    /// A peer can request the attestations for a specific tick data.
    Attestation(B256, oneshot::Sender<Vec<Address>>),
}

/// This struct defines the connection object for the Oracle subprotocol.
pub(crate) struct OracleConnection {
    /// The connection channel receiving RLP bytes from the network.
    conn: ProtocolConnection,
    /// The channel to receive commands from the Oracle network.
    commands: UnboundedReceiverStream<OracleCommand>,
    /// The channel to receive signed ticks from the Oracle network.
    signed_ticks: BroadcastStream<SignedTicker>,
    /// The initial ping message to send to the peer.
    initial_ping: Option<OracleProtoMessage>,
    /// The attestations received from the peer.
    attestations: HashMap<B256, Vec<Address>>,
    /// The pending attestation channel to send back attestations to who requested them.
    pending_att: Option<oneshot::Sender<Vec<Address>>>,
}

impl Stream for OracleConnection {
    type Item = BytesMut;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if let Some(initial_ping) = this.initial_ping.take() {
            return Poll::Ready(Some(initial_ping.encoded()));
        }

        loop {
            if let Poll::Ready(Some(cmd)) = this.commands.poll_next_unpin(cx) {
                let OracleCommand::Attestation(id, pending_att) = cmd;
                let attestations = this.attestations.get(&id).cloned().unwrap_or_default();
                this.pending_att = Some(pending_att);
                return Poll::Ready(Some(OracleProtoMessage::attestations(attestations).encoded()));
            }

            if let Poll::Ready(Some(Ok(tick))) = this.signed_ticks.poll_next_unpin(cx) {
                return Poll::Ready(Some(
                    OracleProtoMessage::signed_ticker(Box::new(tick)).encoded(),
                ));
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
                OracleProtoMessageKind::Attestations(attestations) => {
                    if let Some(sender) = this.pending_att.take() {
                        sender.send(attestations).ok();
                    }
                }
                OracleProtoMessageKind::SignedTicker(signed_data) => {
                    let signer = signed_data.signer;
                    let sig = signed_data.signature;

                    let mut buffer = BytesMut::new();
                    signed_data.ticker.encode(&mut buffer);

                    let addr = match sig.recover_address_from_msg(buffer.clone()) {
                        Ok(addr) => addr,
                        Err(_) => return Poll::Ready(None),
                    };

                    if addr == signer {
                        this.attestations
                            .entry(signed_data.id)
                            .and_modify(|vec| vec.push(addr))
                            .or_insert_with(|| vec![addr]);
                    }

                    let attestations =
                        this.attestations.get(&signed_data.id).cloned().unwrap_or_default();
                    return Poll::Ready(Some(
                        OracleProtoMessage::attestations(attestations).encoded(),
                    ));
                }
            }
        }
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
            signed_ticks: BroadcastStream::new(self.state.to_peers.subscribe()),
            attestations: HashMap::new(),
            pending_att: None,
        }
    }
}
