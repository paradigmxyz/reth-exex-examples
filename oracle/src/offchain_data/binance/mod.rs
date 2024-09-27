use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use feeder::{BinanceDataFeeder, BinanceDataFeederError};
use futures::{FutureExt, Stream, StreamExt};
use reth_tracing::tracing::{error, info};
use ticker::Ticker;

pub(crate) mod feeder;
pub(crate) mod ticker;

/// The reconnection future for the Binance WebSocket connection.
type ReconnectionFuture =
    Pin<Box<dyn Future<Output = Result<BinanceDataFeeder, BinanceDataFeederError>> + Send>>;

/// A struct that manages a Binance WebSocket connection and handles reconnection logic
/// by proxying the underlying `BinanceDataFeeder`.
///
/// The connection is automatically re-established if it encounters a `Connection` error.
pub(crate) struct BinanceConnection {
    pub(crate) inner: BinanceDataFeeder,
    reconnection: Option<ReconnectionFuture>,
}

impl BinanceConnection {
    /// Creates a new `BinanceConnection` with an initial WebSocket connection
    /// to the Binance API using the provided list of symbols.
    pub(crate) async fn new(symbols: Vec<String>) -> Result<Self, BinanceDataFeederError> {
        let inner = BinanceDataFeeder::new(symbols).await?;
        Ok(Self { inner, reconnection: None })
    }
}

impl Stream for BinanceConnection {
    type Item = Result<Ticker, BinanceDataFeederError>;

    /// Polls the next message from the Binance WebSocket connection.
    ///
    /// If the connection is active, it will poll the next message. In case of a connection error,
    /// it will attempt to reconnect by storing a future in `reconnect_future` and polling it until
    /// reconnection is successful.
    ///
    /// # Returns
    /// - `Poll::Ready(Some(Ok(Ticker)))`: If a new ticker message is successfully received.
    /// - `Poll::Ready(Some(Err(BinanceDataFeederError)))`: If there is an error in the WebSocket
    ///   connection.
    /// - `Poll::Ready(None)`: If the WebSocket stream has ended.
    /// - `Poll::Pending`: If the WebSocket stream is still active or a reconnection is in progress.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        let reconn = this.reconnection.take();

        if let Some(mut fut) = reconn {
            match fut.as_mut().poll(cx) {
                Poll::Ready(Ok(conn)) => {
                    this.inner = conn;
                    this.reconnection = None;
                    info!("Reconnected to Binance WebSocket successfully.");
                }
                Poll::Ready(Err(e)) => {
                    info!(?e, "Failed to reconnect to Binance WebSocket.");
                    this.reconnection = None;
                    return Poll::Ready(Some(Err(e)));
                }
                Poll::Pending => {
                    this.reconnection = Some(fut);
                    return Poll::Pending;
                }
            }
        }

        match this.inner.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(msg))) => Poll::Ready(Some(Ok(msg))),
            Poll::Ready(Some(Err(e))) => {
                error!(?e, "Binance WebSocket disconnected. Attempting to reconnect...");
                let fut = BinanceDataFeeder::new(this.inner.symbols.clone()).boxed();
                this.reconnection = Some(fut);
                // make sure we are awaken to poll the reconnection future
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => {
                // wake up the task to poll the stream again
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}
