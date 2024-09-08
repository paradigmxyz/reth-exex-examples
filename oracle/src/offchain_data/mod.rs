use std::pin::Pin;

use binance::BinanceDataFeederError;
use futures::stream::Stream;

pub(crate) mod binance;

/// This trait must be implemented by any data feeder that wants to be used by the Oracle.
///
/// It takes care of feeding off-chain data to the Oracle.
pub(crate) trait DataFeeder {
    /// The Item type that the stream will return.
    type Item;

    /// Converts the data feeder into a stream of items.
    fn into_stream(
        self,
    ) -> Pin<Box<dyn Stream<Item = Result<Self::Item, BinanceDataFeederError>> + Send>>;
}
