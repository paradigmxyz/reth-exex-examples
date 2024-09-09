use futures::{Stream, StreamExt};
use reth_tracing::tracing::error;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use thiserror::Error;
use ticker::{BinanceResponse, Ticker};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

macro_rules! block_on {
    ($expr:expr) => {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on($expr))
    };
}

pub(crate) mod ticker;

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
    inner: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    url: String,
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
        let client = Self::connect_with_retries(url.to_string(), 10).await?;

        Ok(Self { inner: Some(client), url })
    }

    /// Function to connect with retries and an optional delay between retries
    async fn connect_with_retries(
        url: String,
        max_retries: usize,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, BinanceDataFeederError> {
        let mut attempts = 0;

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

        let mut connection = this.inner.take().expect("inner WebSocket client is missing");

        match connection.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(msg))) => {
                let msg = msg.into_text()?;
                let resp: BinanceResponse = serde_json::from_str(&msg)?;
                this.inner = Some(connection);
                Poll::Ready(Some(Ok(resp.data)))
            }
            Poll::Ready(Some(Err(e))) => {
                // handle ws disconnections
                error!(?e, "Binance ws disconnected, reconnecting");
                match block_on!(Self::connect_with_retries(this.url.clone(), 10)) {
                    Ok(conn) => {
                        this.inner = Some(conn);
                        Poll::Pending
                    }
                    Err(reconnect_error) => {
                        error!(?reconnect_error, "Failed to reconnect after max retries");
                        Poll::Ready(Some(Err(reconnect_error)))
                    }
                }
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => {
                this.inner = Some(connection);
                Poll::Pending
            }
        }
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
