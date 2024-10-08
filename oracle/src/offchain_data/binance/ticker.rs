use alloy_rlp::{RlpDecodable, RlpEncodable};
use serde::{Deserialize, Serialize};

/// Struct representing the full JSON response from Binance
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct BinanceResponse {
    /// Stream name (e.g., "ethusdt@ticker")
    stream: String,
    /// The ticker data nested inside the `data` field
    pub data: Ticker,
}

/// Binance ticker data
#[derive(
    Serialize, Deserialize, Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable, Default,
)]
pub(crate) struct Ticker {
    /// Event type (e.g., "24hrTicker")
    #[serde(rename = "e")]
    pub(crate) event_type: String,
    /// Event time (timestamp)
    #[serde(rename = "E")]
    pub(crate) event_time: u64,
    /// Trading pair symbol
    #[serde(rename = "s")]
    pub(crate) symbol: String,
    /// Price change over the last 24 hours
    #[serde(rename = "p")]
    pub(crate) price_change: String,
    /// Price change percent
    #[serde(rename = "P")]
    pub(crate) price_change_percent: String,
    /// Weighted average price
    #[serde(rename = "w")]
    pub(crate) weighted_avg_price: String,
    /// Previous day's close price
    #[serde(rename = "x")]
    pub(crate) prev_close_price: String,
    /// Current price (last trade price)
    #[serde(rename = "c")]
    pub(crate) last_price: String,
    /// Last quantity traded
    #[serde(rename = "Q")]
    pub(crate) last_quantity: String,
    /// Best bid price
    #[serde(rename = "b")]
    pub(crate) best_bid_price: String,
    /// Best bid quantity
    #[serde(rename = "B")]
    pub(crate) best_bid_quantity: String,
    /// Best ask price
    #[serde(rename = "a")]
    pub(crate) best_ask_price: String,
    /// Best ask quantity
    #[serde(rename = "A")]
    pub(crate) best_ask_quantity: String,
    /// Open price for the 24-hour period
    #[serde(rename = "o")]
    pub(crate) open_price: String,
    /// High price of the 24-hour period
    #[serde(rename = "h")]
    pub(crate) high_price: String,
    /// Low price of the 24-hour period
    #[serde(rename = "l")]
    pub(crate) low_price: String,
    /// Total traded volume of the base asset
    #[serde(rename = "v")]
    pub(crate) volume: String,
    /// Total traded volume of the quote asset
    #[serde(rename = "q")]
    pub(crate) quote_volume: String,
    /// Open time (timestamp)
    #[serde(rename = "O")]
    pub(crate) open_time: u64,
    /// Close time (timestamp)
    #[serde(rename = "C")]
    pub(crate) close_time: u64,
    /// First trade ID
    #[serde(rename = "F")]
    pub(crate) first_trade_id: u64,
    /// Last trade ID
    #[serde(rename = "L")]
    pub(crate) last_trade_id: u64,
    /// Total number of trades
    #[serde(rename = "n")]
    pub(crate) num_trades: u64,
}
