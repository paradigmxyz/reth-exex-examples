//! Types for the P2P network.

use kona_derive::types::L2ExecutionPayload;
use alloy::primitives::{Signature, keccak256, B256};

/// An envelope around the execution payload for L2.
#[derive(Debug)]
pub struct ExecutionPayloadEnvelope {
    /// The execution payload.
    pub payload: L2ExecutionPayload,
    /// A signature for the payload.
    pub signature: Signature,
    /// The hash of the payload.
    pub hash: PayloadHash,
    /// The parent beacon block root.
    pub parent_beacon_block_root: Option<B256>,
}

/// Represents the Keccak256 hash of the block
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadHash(B256);

impl From<&[u8]> for PayloadHash {
    /// Returns the Keccak256 hash of a sequence of bytes
    fn from(value: &[u8]) -> Self {
        Self(keccak256(value).into())
    }
}

impl PayloadHash {
    /// The expected message that should be signed by the unsafe block signer.
    pub fn signature_message(&self, chain_id: u64) -> B256 {
        let domain = B256::ZERO;
        let chain_id = B256::from_slice(&chain_id.to_be_bytes()[..]);
        let payload_hash = self.0;

        let data: Vec<u8> = [
            domain.as_slice(),
            chain_id.as_slice(),
            payload_hash.as_slice(),
        ]
        .concat();

        keccak256(data).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::b256;

    #[test]
    fn test_signature_message() {
        let inner = b256!("9999999999999999999999999999999999999999999999999999999999999999");
        let hash = PayloadHash::from(inner.as_slice());
        let chain_id = 10;
        let expected = b256!("9999999999999999999999999999999999999999999999999999999999999999");
        assert_eq!(hash.signature_message(chain_id), expected);
    }
}
