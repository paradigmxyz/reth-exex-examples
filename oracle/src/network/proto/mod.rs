#![allow(dead_code)]

use alloy_primitives::Address;
use alloy_rlp::{Buf, BufMut, BytesMut, Decodable, Encodable};
use connection::{OracleCommand, OracleConnHandler};
use data::SignedTicker;
use reth_eth_wire::{protocol::Protocol, Capability};
use reth_network::{protocol::ProtocolHandler, Direction};
use reth_network_api::PeerId;
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
    Attestations = 0x03,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OracleProtoMessageKind {
    Ping,
    Pong,
    SignedTicker(Box<SignedTicker>),
    Attestations(Vec<Address>),
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

    /// Creates an attestations message
    pub(crate) fn attestations(attestations: Vec<Address>) -> Self {
        Self {
            message_type: OracleProtoMessageId::Attestations,
            message: OracleProtoMessageKind::Attestations(attestations),
        }
    }

    /// Creates a signed ticker message
    pub(crate) fn signed_ticker(data: Box<SignedTicker>) -> Self {
        Self {
            message_type: OracleProtoMessageId::TickData,
            message: OracleProtoMessageKind::SignedTicker(data),
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
            OracleProtoMessageKind::Attestations(data) => {
                data.encode(&mut buf);
            }
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
            0x03 => OracleProtoMessageId::Attestations,
            _ => return None,
        };
        let message = match message_type {
            OracleProtoMessageId::Ping => OracleProtoMessageKind::Ping,
            OracleProtoMessageId::Pong => OracleProtoMessageKind::Pong,
            OracleProtoMessageId::Attestations => {
                let data = Vec::<Address>::decode(buf).ok()?;
                OracleProtoMessageKind::Attestations(data)
            }
            OracleProtoMessageId::TickData => {
                let data = SignedTicker::decode(buf).ok()?;
                OracleProtoMessageKind::SignedTicker(Box::new(data))
            }
        };

        Some(Self { message_type, message })
    }
}

pub(crate) type ProtoEvents = mpsc::UnboundedReceiver<ProtocolEvent>;
pub(crate) type ToPeers = tokio::sync::broadcast::Sender<SignedTicker>;

/// This struct is responsible of incoming and outgoing connections.
#[derive(Debug)]
pub(crate) struct OracleProtoHandler {
    pub(crate) state: ProtocolState,
}

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

impl OracleProtoHandler {
    /// Creates a new `OracleProtoHandler` with the given protocol state.
    pub(crate) fn new() -> (Self, ProtoEvents, ToPeers) {
        let (tx, rx) = mpsc::unbounded_channel();
        let (to_peers, _) = tokio::sync::broadcast::channel(BROADCAST_CHANNEL_SIZE);
        (Self { state: ProtocolState { events: tx, to_peers: to_peers.clone() } }, rx, to_peers)
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
    pub(crate) to_peers: tokio::sync::broadcast::Sender<SignedTicker>,
}

/// The events that can be emitted by our custom protocol.
#[derive(Debug, Clone)]
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
    use alloy_primitives::B256;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
    use mpsc::{UnboundedReceiver, UnboundedSender};
    use reth::providers::test_utils::MockEthProvider;
    use reth_network::test_utils::Testnet;
    use tokio::sync::oneshot;

    #[tokio::test(flavor = "multi_thread")]
    async fn can_attest_multiple_times() {
        reth_tracing::init_test_tracing();
        let provider = MockEthProvider::default();
        let mut net = Testnet::create_with(2, provider.clone()).await;

        let (events, mut from_peer0) = mpsc::unbounded_channel();
        let (to_peer0, mut broadcast_from_peer0) =
            tokio::sync::broadcast::channel(BROADCAST_CHANNEL_SIZE);
        net.peers_mut()[0].add_rlpx_sub_protocol(OracleProtoHandler {
            state: ProtocolState { events, to_peers: to_peer0.clone() },
        });

        let (events, mut from_peer1) = mpsc::unbounded_channel();
        let (to_peer1, mut broadcast_from_peer1) =
            tokio::sync::broadcast::channel(BROADCAST_CHANNEL_SIZE);
        net.peers_mut()[1].add_rlpx_sub_protocol(OracleProtoHandler {
            state: ProtocolState { events, to_peers: to_peer1.clone() },
        });

        let handle = net.spawn();

        handle.connect_peers().await;

        let peer0_conn = established(&mut from_peer0, *handle.peers()[1].peer_id()).await;
        let peer1_conn = established(&mut from_peer1, *handle.peers()[0].peer_id()).await;

        let (peer0_att, _, broadcast_from_peer0) =
            sent_att(&to_peer0, &mut broadcast_from_peer0).await;
        got_att(peer0_conn.clone(), peer0_att.id, peer0_att.signer, 1).await;

        let (peer1_att, _, _) = sent_att(&to_peer1, &mut broadcast_from_peer1).await;
        got_att(peer1_conn.clone(), peer1_att.id, peer1_att.signer, 1).await;

        let (att, _to_peer0, _broad_cast_from_peer0) =
            sent_att(&to_peer0, broadcast_from_peer0).await;
        got_att(peer0_conn, att.id, att.signer, 2).await;
    }

    async fn established(
        from_peer0: &mut UnboundedReceiver<ProtocolEvent>,
        wanted: PeerId,
    ) -> UnboundedSender<OracleCommand> {
        let peer0_to_peer1 = from_peer0.recv().await.unwrap();
        match peer0_to_peer1 {
            ProtocolEvent::Established { direction: _, peer_id, to_connection } => {
                assert_eq!(wanted, peer_id);
                to_connection
            }
        }
    }
    async fn got_att(
        connection: UnboundedSender<OracleCommand>,
        attestation_id: B256,
        expected_signer: Address,
        expcted_att_len: usize,
    ) {
        let (tx, rx) = oneshot::channel();

        connection.send(OracleCommand::Attestation(attestation_id, tx)).unwrap();

        let response = rx.await.unwrap();
        assert_eq!(response.len(), expcted_att_len);
        assert_eq!(response[expcted_att_len - 1], expected_signer);
    }

    async fn sent_att<'a>(
        to_peer: &'a tokio::sync::broadcast::Sender<SignedTicker>,
        broadcast_from_peer: &'a mut tokio::sync::broadcast::Receiver<SignedTicker>,
    ) -> (
        SignedTicker,
        &'a tokio::sync::broadcast::Sender<SignedTicker>,
        &'a mut tokio::sync::broadcast::Receiver<SignedTicker>,
    ) {
        // Create a new signer and sign the ticker data
        let signer = PrivateKeySigner::random();
        let signer_address = signer.address();
        let ticker_data = mock_ticker();
        let mut buffer = BytesMut::new();
        ticker_data.encode(&mut buffer);
        let signature = signer.sign_message_sync(&buffer).unwrap();

        // Create the signed ticker
        let signed_ticker = SignedTicker::new(ticker_data, signature, signer_address);

        // Send the signed ticker to the peer
        to_peer.send(signed_ticker.clone()).unwrap();

        // Expect it in the broadcast receiver
        let received = broadcast_from_peer.recv().await.unwrap();
        assert_eq!(received, signed_ticker);

        (signed_ticker, to_peer, broadcast_from_peer)
    }
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
