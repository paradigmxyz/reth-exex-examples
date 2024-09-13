use crate::offchain_data::binance::ticker::Ticker;
use alloy_rlp::{RlpDecodable, RlpEncodable};
use alloy_signer::Signature;
use reth_primitives::Address;

#[derive(Clone, Debug, RlpEncodable, RlpDecodable, PartialEq)]
pub struct SignedTicker {
    pub(crate) ticker: Ticker,
    pub(crate) signature: Signature,
    pub(crate) signer: Address,
}

impl SignedTicker {
    pub fn new(ticker: Ticker, signature: Signature, signer: Address) -> Self {
        Self { ticker, signature, signer }
    }
}
