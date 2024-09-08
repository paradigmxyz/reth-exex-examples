use serde::{Deserialize, Serialize};

/// Binance ticker data
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Ticker {
    /// Event type (e.g., "24hrTicker")
    #[serde(rename = "e")]
    event_type: String,
    /// Event time (timestamp)
    #[serde(rename = "E")]
    event_time: u64,
    /// Trading pair symbol
    #[serde(rename = "s")]
    symbol: String,
    /// Price change over the last 24 hours
    #[serde(rename = "p")]
    price_change: String,
    /// Price change percent
    #[serde(rename = "P")]
    price_change_percent: String,
    /// Weighted average price
    #[serde(rename = "w")]
    weighted_avg_price: String,
    /// Previous day's close price
    #[serde(rename = "x")]
    prev_close_price: String,
    /// Current price (last trade price)
    #[serde(rename = "c")]
    last_price: String,
    /// Last quantity traded
    #[serde(rename = "Q")]
    last_quantity: String,
    /// Best bid price
    #[serde(rename = "b")]
    best_bid_price: String,
    /// Best bid quantity
    #[serde(rename = "B")]
    best_bid_quantity: String,
    /// Best ask price
    #[serde(rename = "a")]
    best_ask_price: String,
    /// Best ask quantity
    #[serde(rename = "A")]
    best_ask_quantity: String,
    /// Open price for the 24-hour period
    #[serde(rename = "o")]
    open_price: String,
    /// High price of the 24-hour period
    #[serde(rename = "h")]
    high_price: String,
    /// Low price of the 24-hour period
    #[serde(rename = "l")]
    low_price: String,
    /// Total traded volume of the base asset
    #[serde(rename = "v")]
    volume: String,
    /// Total traded volume of the quote asset
    #[serde(rename = "q")]
    quote_volume: String,
    /// Open time (timestamp)
    #[serde(rename = "O")]
    open_time: u64,
    /// Close time (timestamp)
    #[serde(rename = "C")]
    close_time: u64,
    /// First trade ID
    #[serde(rename = "F")]
    first_trade_id: u64,
    /// Last trade ID
    #[serde(rename = "L")]
    last_trade_id: u64,
    /// Total number of trades
    #[serde(rename = "n")]
    num_trades: u64,
}

pub(crate) struct BinanceDataFeeder {
    symbols: Vec<String>,
}

impl BinanceDataFeeder {
    pub(crate) fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
    async fn connect(
        &self,
    ) -> tokio_tungstenite::tungstenite::Result<
        tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    > {
        let url = format!("wss://stream.binance.com:9443/ws/{}@ticker", self.symbol.to_lowercase());
        let (ws_stream, _) = connect_async(url).await?;
        Ok(ws_stream)
    }
}
