use crate::offchain_data::binance::ticker::Ticker;
use alloy_primitives::{keccak256, Address, Bytes, B256};
use alloy_rlp::{BytesMut, Encodable, RlpDecodable, RlpEncodable};
use alloy_signer::Signature;

#[derive(Clone, Debug, RlpEncodable, RlpDecodable, PartialEq)]
pub struct SignedTicker {
    pub(crate) ticker: Ticker,
    pub(crate) signature: Bytes,
    pub(crate) signer: Address,
    pub(crate) id: B256,
}

impl SignedTicker {
    pub fn new(ticker: Ticker, signature: Signature, signer: Address) -> Self {
        let mut buffer = BytesMut::new();
        ticker.encode(&mut buffer);
        let id = keccak256(&buffer);
        Self { ticker, signature: signature.as_bytes().into(), signer, id }
    }
}
