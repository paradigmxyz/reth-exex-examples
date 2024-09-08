use futures::stream::Stream;
use serde::de::DeserializeOwned;

pub(crate) mod binance;

/// This trait must be implemented by any data feeder that wants to be used by the Oracle.
pub(crate) trait DataFeeder {
    /// The `into_stream` method should return a stream of items that implement
    /// `serde::Deserialize`. This uses `DeserializeOwned` for deserialization of the stream
    /// items.
    fn into_stream<T>(&self) -> Box<dyn Stream<Item = T> + Unpin>
    where
        T: DeserializeOwned + 'static;
}
