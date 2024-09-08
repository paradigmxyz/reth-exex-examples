use futures::{Stream, StreamExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use thiserror::Error;
use ticker::{BinanceResponse, Ticker};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use super::DataFeeder;

mod ticker;

#[derive(Error, Debug)]
pub(crate) enum BinanceDataFeederError {
    /// Error connecting to the WebSocket.
    #[error("error connecting to WebSocket")]
    Connection(#[from] tokio_tungstenite::tungstenite::Error),
    /// Error decoding the message.
    #[error("error decoding message")]
    Decode(#[from] serde_json::Error),
}

/// This structure controls the interaction with the Binance WebSocket API.
pub(crate) struct BinanceDataFeeder {
    /// The URL of the Binance WebSocket API.
    url: String,
    /// The WebSocket stream.
    inner: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl BinanceDataFeeder {
    /// Creates a new BinanceDataFeeder instance.
    pub(crate) async fn new(symbols: Vec<String>) -> Result<Self, BinanceDataFeederError> {
        let query = symbols
            .iter()
            .map(|symbol| format!("{}@ticker", symbol))
            .collect::<Vec<String>>()
            .join("/");

        let url = format!("wss://stream.binance.com/stream?streams={}", query);
        let (client, _) = connect_async(url.to_string()).await?;

        Ok(Self { url, inner: client })
    }
}

/// We implement the Stream trait for the BinanceDataFeeder struct
/// in order to encode the messages received from the WebSocket into our Ticker struct.
impl Stream for BinanceDataFeeder {
    type Item = Result<Ticker, BinanceDataFeederError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        match this.inner.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(msg))) => {
                let msg = msg.into_text()?;
                let resp: BinanceResponse = serde_json::from_str(&msg)?;
                Poll::Ready(Some(Ok(resp.data)))
            }
            Poll::Ready(Some(Err(e))) => {
                // we should handle reconnections here
                Poll::Ready(Some(Err(BinanceDataFeederError::Connection(e))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl DataFeeder for BinanceDataFeeder {
    type Item = Ticker;

    /// Converts the Binance feeder into a stream of `Ticker` items.
    fn into_stream(
        self,
    ) -> Pin<Box<dyn Stream<Item = Result<Self::Item, BinanceDataFeederError>> + Send>> {
        Box::pin(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn can_connect() {
        let symbols = vec!["btcusdt".to_string(), "ethusdt".to_string()];
        let mut feeder = BinanceDataFeeder::new(symbols).await.unwrap();
        let msg = feeder.next().await.unwrap().unwrap();
        assert!(msg.symbol == "BTCUSDT" || msg.symbol == "ETHUSDT");
    }
}
