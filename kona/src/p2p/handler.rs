//! Block Handler

use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::SystemTime;
use alloy::primitives::Address;
use libp2p::gossipsub::{IdentTopic, Message, MessageAcceptance, TopicHash};
// use ssz_rs::{prelude::*, List, Vector, U256};
use tokio::sync::watch;

use crate::p2p::types::ExecutionPayloadEnvelope;

/// This trait defines the functionality required to process incoming messages
/// and determine their acceptance within the network. Implementors of this trait
/// can specify how messages are handled and which topics they are interested in.
pub trait Handler: Send {
    /// Manages validation and further processing of messages
    fn handle(&self, msg: Message) -> MessageAcceptance;

    /// Specifies which topics the handler is interested in
    fn topics(&self) -> Vec<TopicHash>;
}

/// Responsible for managing blocks received via p2p gossip
pub struct BlockHandler {
    /// Chain ID of the L2 blockchain. Used to filter out gossip messages intended for other blockchains.
    chain_id: u64,
    /// A channel sender to forward new blocks to other modules
    block_sender: Sender<ExecutionPayloadEnvelope>,
    /// A [watch::Receiver] to monitor changes to the unsafe block signer.
    unsafe_signer_recv: watch::Receiver<Address>,
    /// The libp2p topic for pre Canyon/Shangai blocks.
    blocks_v1_topic: IdentTopic,
    /// The libp2p topic for Canyon/Delta blocks.
    blocks_v2_topic: IdentTopic,
    /// The libp2p topic for Ecotone V3 blocks.
    blocks_v3_topic: IdentTopic,
}

impl Handler for BlockHandler {
    /// Checks validity of a block received via p2p gossip, and sends to the block update channel if valid.
    fn handle(&self, msg: Message) -> MessageAcceptance {
        tracing::debug!("received block");

        let _decoded = if msg.topic == self.blocks_v1_topic.hash() {
            // decode_pre_ecotone_block_msg::<ExecutionPayloadV1SSZ>(msg.data)
            unimplemented!()
        } else if msg.topic == self.blocks_v2_topic.hash() {
            // decode_pre_ecotone_block_msg::<ExecutionPayloadV2SSZ>(msg.data)
            unimplemented!()
        } else if msg.topic == self.blocks_v3_topic.hash() {
            // decode_post_ecotone_block_msg(msg.data)
            unimplemented!()
        } else {
            return MessageAcceptance::Reject;
        };

        // match decoded {
        //     Ok(envelope) => {
        //         if self.block_valid(&envelope) {
        //             _ = self.block_sender.send(envelope.payload);
        //             MessageAcceptance::Accept
        //         } else {
        //             tracing::warn!("invalid unsafe block");
        //             MessageAcceptance::Reject
        //         }
        //     }
        //     Err(err) => {
        //         tracing::warn!("unsafe block decode failed: {}", err);
        //         MessageAcceptance::Reject
        //     }
        // }
    }

    /// The gossip topics accepted for new blocks
    fn topics(&self) -> Vec<TopicHash> {
        vec![self.blocks_v1_topic.hash(), self.blocks_v2_topic.hash()]
    }
}

impl BlockHandler {
    /// Creates a new [BlockHandler] and opens a channel
    pub fn new(chain_id: u64, unsafe_recv: watch::Receiver<Address>) -> (Self, Receiver<ExecutionPayloadEnvelope>) {
        let (sender, recv) = channel();

        let handler = Self {
            chain_id,
            block_sender: sender,
            unsafe_signer_recv: unsafe_recv,
            blocks_v1_topic: IdentTopic::new(format!("/optimism/{}/0/blocks", chain_id)),
            blocks_v2_topic: IdentTopic::new(format!("/optimism/{}/1/blocks", chain_id)),
            blocks_v3_topic: IdentTopic::new(format!("/optimism/{}/2/blocks", chain_id)),
        };

        (handler, recv)
    }

    /// Determines if a block is valid.
    ///
    /// True if the block is less than 1 minute old, and correctly signed by the unsafe block signer.
    fn block_valid(&self, envelope: &ExecutionPayloadEnvelope) -> bool {
        let current_timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let is_future = envelope.payload.timestamp > current_timestamp + 5;
        let is_past = envelope.payload.timestamp < current_timestamp - 60;
        let time_valid = !(is_future || is_past);

        let msg = envelope.hash.signature_message(self.chain_id);
        let block_signer = *self.unsafe_signer_recv.borrow();
        let Ok(msg_signer) = envelope.signature.recover_address_from_msg(msg) else {
            // TODO: add telemetry here if this happens.
            return false;
        };

        time_valid && msg_signer == block_signer
    }
}
