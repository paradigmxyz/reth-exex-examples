#![allow(dead_code)]

use connection::{OracleCommand, OracleConnHandler};
use reth_eth_wire::{protocol::Protocol, Capability};
use reth_network::{protocol::ProtocolHandler, Direction};
use reth_network_api::PeerId;
use reth_primitives::{Buf, BufMut, BytesMut};
use std::net::SocketAddr;
use tokio::sync::mpsc;

pub(crate) mod connection;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OracleProtoMessageId {
    Ping = 0x00,
    Pong = 0x01,
    PingMessage = 0x02,
    PongMessage = 0x03,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OracleProtoMessageKind {
    Ping,
    Pong,
    PingMessage(String),
    PongMessage(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OracleProtoMessage {
    pub(crate) message_type: OracleProtoMessageId,
    pub(crate) message: OracleProtoMessageKind,
}

impl OracleProtoMessage {
    /// Returns the capability for the `custom_rlpx` protocol.
    pub(crate) fn capability() -> Capability {
        Capability::new_static("custom_rlpx", 1)
    }

    /// Returns the protocol for the `custom_rlpx` protocol.
    pub(crate) fn protocol() -> Protocol {
        Protocol::new(Self::capability(), 4)
    }

    /// Creates a ping message
    pub(crate) fn ping_message(msg: impl Into<String>) -> Self {
        Self {
            message_type: OracleProtoMessageId::PingMessage,
            message: OracleProtoMessageKind::PingMessage(msg.into()),
        }
    }
    /// Creates a ping message
    pub(crate) fn pong_message(msg: impl Into<String>) -> Self {
        Self {
            message_type: OracleProtoMessageId::PongMessage,
            message: OracleProtoMessageKind::PongMessage(msg.into()),
        }
    }

    /// Creates a ping message
    pub(crate) fn ping() -> Self {
        Self { message_type: OracleProtoMessageId::Ping, message: OracleProtoMessageKind::Ping }
    }

    /// Creates a pong message
    pub(crate) fn pong() -> Self {
        Self { message_type: OracleProtoMessageId::Pong, message: OracleProtoMessageKind::Pong }
    }

    /// Creates a new `OracleProtoMessage` with the given message ID and payload.
    pub(crate) fn encoded(&self) -> BytesMut {
        let mut buf = BytesMut::new();
        buf.put_u8(self.message_type as u8);
        match &self.message {
            OracleProtoMessageKind::Ping | OracleProtoMessageKind::Pong => {}
            OracleProtoMessageKind::PingMessage(msg) | OracleProtoMessageKind::PongMessage(msg) => {
                buf.put(msg.as_bytes());
            }
        }
        buf
    }

    /// Decodes a `OracleProtoMessage` from the given message buffer.
    pub(crate) fn decode_message(buf: &mut &[u8]) -> Option<Self> {
        if buf.is_empty() {
            return None;
        }
        let id = buf[0];
        buf.advance(1);
        let message_type = match id {
            0x00 => OracleProtoMessageId::Ping,
            0x01 => OracleProtoMessageId::Pong,
            0x02 => OracleProtoMessageId::PingMessage,
            0x03 => OracleProtoMessageId::PongMessage,
            _ => return None,
        };
        let message = match message_type {
            OracleProtoMessageId::Ping => OracleProtoMessageKind::Ping,
            OracleProtoMessageId::Pong => OracleProtoMessageKind::Pong,
            OracleProtoMessageId::PingMessage => {
                OracleProtoMessageKind::PingMessage(String::from_utf8_lossy(&buf[..]).into_owned())
            }
            OracleProtoMessageId::PongMessage => {
                OracleProtoMessageKind::PongMessage(String::from_utf8_lossy(&buf[..]).into_owned())
            }
        };

        Some(Self { message_type, message })
    }
}

/// This struct is responsible of incoming and outgoing connections.
#[derive(Debug)]
pub(crate) struct OracleProtoHandler {
    state: ProtocolState,
}

impl OracleProtoHandler {
    /// Creates a new `OracleProtoHandler` with the given protocol state.
    pub(crate) fn new() -> Self {
        let (tx, _) = mpsc::unbounded_channel();
        Self { state: ProtocolState { events: tx } }
    }
}

impl ProtocolHandler for OracleProtoHandler {
    type ConnectionHandler = OracleConnHandler;

    fn on_incoming(&self, _socket_addr: SocketAddr) -> Option<Self::ConnectionHandler> {
        Some(OracleConnHandler { state: self.state.clone() })
    }

    fn on_outgoing(
        &self,
        _socket_addr: SocketAddr,
        _peer_id: PeerId,
    ) -> Option<Self::ConnectionHandler> {
        Some(OracleConnHandler { state: self.state.clone() })
    }
}

/// Protocol state is an helper struct to store the protocol events.
#[derive(Clone, Debug)]
pub(crate) struct ProtocolState {
    pub(crate) events: mpsc::UnboundedSender<ProtocolEvent>,
}

/// The events that can be emitted by our custom protocol.
#[derive(Debug)]
pub(crate) enum ProtocolEvent {
    Established {
        direction: Direction,
        peer_id: PeerId,
        to_connection: mpsc::UnboundedSender<OracleCommand>,
    },
}
