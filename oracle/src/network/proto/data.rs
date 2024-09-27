use crate::offchain_data::binance::ticker::Ticker;
use alloy_rlp::{BytesMut, Encodable, RlpDecodable, RlpEncodable};
use alloy_signer::Signature;
use reth_primitives::{keccak256, Address, B256};

#[derive(Clone, Debug, RlpEncodable, RlpDecodable, PartialEq)]
pub struct SignedTicker {
    pub(crate) ticker: Ticker,
    pub(crate) signature: Signature,
    pub(crate) signer: Address,
    pub(crate) id: B256,
}

impl SignedTicker {
    pub fn new(ticker: Ticker, signature: Signature, signer: Address) -> Self {
        let mut buffer = BytesMut::new();
        ticker.encode(&mut buffer);
        let id = keccak256(&buffer);
        Self { ticker, signature, signer, id }
    }
}
