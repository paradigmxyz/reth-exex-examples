#![allow(dead_code)]

use alloy_rlp::{Decodable, Encodable};
use connection::{OracleCommand, OracleConnHandler};
use data::SignedTicker;
use reth_eth_wire::{protocol::Protocol, Capability};
use reth_network::{protocol::ProtocolHandler, Direction};
use reth_network_api::PeerId;
use reth_primitives::{Buf, BufMut, BytesMut};
use std::net::SocketAddr;
use tokio::sync::mpsc;

pub(crate) mod connection;
pub(crate) mod data;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OracleProtoMessageId {
    Ping = 0x00,
    Pong = 0x01,
    TickData = 0x02,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OracleProtoMessageKind {
    Ping,
    Pong,
    SignedTicker(Box<SignedTicker>),
}

#[derive(Clone, Debug)]
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

    /// Creates a signed ticker message
    pub(crate) fn signed_ticker(data: SignedTicker) -> Self {
        Self {
            message_type: OracleProtoMessageId::TickData,
            message: OracleProtoMessageKind::SignedTicker(Box::new(data)),
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
            OracleProtoMessageKind::SignedTicker(data) => {
                data.encode(&mut buf);
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
            0x02 => OracleProtoMessageId::TickData,
            _ => return None,
        };
        let message = match message_type {
            OracleProtoMessageId::Ping => OracleProtoMessageKind::Ping,
            OracleProtoMessageId::Pong => OracleProtoMessageKind::Pong,
            OracleProtoMessageId::TickData => {
                let data = SignedTicker::decode(buf).ok()?;
                OracleProtoMessageKind::SignedTicker(Box::new(data))
            }
        };

        Some(Self { message_type, message })
    }
}

/// This struct is responsible of incoming and outgoing connections.
#[derive(Debug)]
pub(crate) struct OracleProtoHandler {
    pub(crate) state: ProtocolState,
}

pub(crate) type ProtoEvents = mpsc::UnboundedReceiver<ProtocolEvent>;

impl OracleProtoHandler {
    /// Creates a new `OracleProtoHandler` with the given protocol state.
    pub(crate) fn new() -> (Self, ProtoEvents) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { state: ProtocolState { events: tx } }, rx)
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::offchain_data::binance::ticker::Ticker;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;

    #[test]
    fn can_decode_msg() {
        test_msg(OracleProtoMessage::ping());
        test_msg(OracleProtoMessage::pong());

        // generate signer
        let signer = PrivateKeySigner::random();
        let signer_address = signer.address();

        // sign ticker data
        let ticker_data = mock_ticker();
        let mut buffer = BytesMut::new();
        ticker_data.encode(&mut buffer);
        let signature = signer.sign_message_sync(&buffer).unwrap();

        // recover signer address and verify it was signed properly
        let recovered_addr = signature.recover_address_from_msg(&buffer).unwrap();
        assert_eq!(recovered_addr, signer_address);

        // create signed ticker
        let signed_ticker = SignedTicker::new(ticker_data, signature, signer_address);

        // test the RLP encoding/decoding
        test_msg(OracleProtoMessage {
            message_type: OracleProtoMessageId::TickData,
            message: OracleProtoMessageKind::SignedTicker(Box::new(signed_ticker)),
        });
    }

    fn test_msg(message: OracleProtoMessage) {
        let encoded = message.encoded();
        let decoded = OracleProtoMessage::decode_message(&mut &encoded[..]).unwrap();
        assert_eq!(message.message_type, decoded.message_type);
        assert_eq!(message.message, decoded.message);
    }

    fn mock_ticker() -> Ticker {
        Ticker {
            event_type: "24hrTicker".to_string(),
            event_time: 1622548800000,
            symbol: "BTCUSDT".to_string(),
            price_change: "100.0".to_string(),
            price_change_percent: "1.0".to_string(),
            weighted_avg_price: "50000.0".to_string(),
            prev_close_price: "49000.0".to_string(),
            last_price: "50000.0".to_string(),
            last_quantity: "0.1".to_string(),
            best_bid_price: "49950.0".to_string(),
            best_bid_quantity: "0.2".to_string(),
            best_ask_price: "50050.0".to_string(),
            best_ask_quantity: "0.3".to_string(),
            open_price: "49000.0".to_string(),
            high_price: "51000.0".to_string(),
            low_price: "48000.0".to_string(),
            volume: "1000.0".to_string(),
            quote_volume: "50000000.0".to_string(),
            open_time: 1622462400000,
            close_time: 1622548800000,
            first_trade_id: 100000,
            last_trade_id: 100100,
            num_trades: 100,
        }
    }
}
