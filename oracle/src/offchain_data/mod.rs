use binance::{feeder::BinanceDataFeederError, ticker::Ticker, BinanceConnection};
use futures::{stream::Stream, StreamExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use thiserror::Error;

pub(crate) mod binance;

/// The enum that represents the various types of data feeds, e.g., Binance.
pub(crate) enum DataFeeds {
    Binance(Ticker),
}

/// The error enum that wraps errors from all data feeders.
#[derive(Error, Debug)]
pub(crate) enum DataFeederError {
    #[error(transparent)]
    Binance(#[from] BinanceDataFeederError),
}

/// The struct that implements the Stream trait for polling multiple data feeds.
pub(crate) struct DataFeederStream {
    binance: BinanceConnection,
    // Add other feeder fields if needed.
}

impl DataFeederStream {
    /// Creates a new instance of the DataFeederStream with initialized Binance feeder.
    pub(crate) async fn new(binance_symbols: Vec<String>) -> Result<Self, DataFeederError> {
        let binance = BinanceConnection::new(binance_symbols).await?;
        Ok(Self { binance })
    }
}

/// Implementing the Stream trait for DataFeederStream to wrap the individual feeders.
impl Stream for DataFeederStream {
    type Item = Result<DataFeeds, DataFeederError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        match this.binance.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(ticker))) => Poll::Ready(Some(Ok(DataFeeds::Binance(ticker)))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(DataFeederError::Binance(e)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }

        // Add other data-feeds here.
    }
}
