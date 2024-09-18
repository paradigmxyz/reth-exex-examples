use futures::{ready, Stream, StreamExt};
use reth_tracing::tracing::error;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use super::ticker::{BinanceResponse, Ticker};

#[derive(Error, Debug)]
pub(crate) enum BinanceDataFeederError {
    /// Error connecting to the WebSocket.
    #[error("error connecting to WebSocket")]
    Connection(#[from] tokio_tungstenite::tungstenite::Error),
    /// Error decoding the message.
    #[error("error decoding message")]
    Decode(#[from] serde_json::Error),
    /// Error reconnecting after max retries.
    #[error("reached max number of reconnection attempts")]
    MaxRetriesExceeded,
}

/// This structure controls the interaction with the Binance WebSocket API.
pub(crate) struct BinanceDataFeeder {
    /// The WebSocket stream.
    pub(crate) inner: WebSocketStream<MaybeTlsStream<TcpStream>>,
    pub(crate) symbols: Vec<String>,
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
        let client = Self::connect_with_retries(url.to_string()).await?;

        Ok(Self { inner: client, symbols })
    }

    /// Function to connect with retries and an optional delay between retries
    pub(crate) async fn connect_with_retries(
        url: String,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, BinanceDataFeederError> {
        let mut attempts = 0;
        let max_retries = 10;

        while attempts < max_retries {
            let conn = connect_async(url.clone()).await;

            match conn {
                Ok((connection, _)) => return Ok(connection),
                Err(e) => {
                    error!(?e, attempts, max_retries, "Connection attempt failed, retrying...");
                    attempts += 1;
                    if attempts < max_retries {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }

        Err(BinanceDataFeederError::MaxRetriesExceeded)
    }
}

/// We implement the Stream trait for the BinanceDataFeeder struct
/// in order to encode the messages received from the WebSocket into our Ticker struct.
impl Stream for BinanceDataFeeder {
    type Item = Result<Ticker, BinanceDataFeederError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        match ready!(this.inner.poll_next_unpin(cx)) {
            Some(Ok(msg)) => {
                let msg = match msg.into_text() {
                    Ok(text) => text,
                    Err(e) => {
                        error!(?e, "Failed to convert message to text, skipping");
                        return Poll::Pending;
                    }
                };

                let msg = match serde_json::from_str::<BinanceResponse>(&msg) {
                    Ok(msg) => msg,
                    Err(e) => {
                        error!(?e, ?msg, "Failed to decode message, skipping");
                        return Poll::Pending;
                    }
                };

                Poll::Ready(Some(Ok(msg.data)))
            }
            Some(Err(e)) => {
                error!(?e, "Binance ws disconnected, reconnecting");
                Poll::Ready(Some(Err(BinanceDataFeederError::Connection(e))))
            }
            None => Poll::Ready(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[ignore]
    #[tokio::test]
    async fn can_connect() {
        let symbols = vec!["btcusdt".to_string(), "ethusdt".to_string()];
        let mut feeder = BinanceDataFeeder::new(symbols).await.unwrap();
        let msg = feeder.next().await.unwrap().unwrap();
        assert!(msg.symbol == "BTCUSDT" || msg.symbol == "ETHUSDT");
    }
}
