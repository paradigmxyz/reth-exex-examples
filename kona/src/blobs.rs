//! Blob Provider

use std::{boxed::Box, sync::Arc};

use async_trait::async_trait;
use hashmore::FIFOMap;
use kona_derive::{
    online::{
        OnlineBeaconClient, OnlineBlobProviderBuilder, OnlineBlobProviderWithFallback,
        SimpleSlotDerivation,
    },
    traits::BlobProvider,
    types::{alloy_primitives::B256, Blob, BlobProviderError, BlockInfo, IndexedBlobHash},
};
use parking_lot::Mutex;
use reqwest::Url;
use tracing::warn;

/// [BlobProvider] for the Kona derivation pipeline.
#[derive(Debug, Clone)]
pub struct ExExBlobProvider {
    /// In-memory blob provider, used for locally caching blobs as
    /// they come during live sync (when following the chain tip).
    memory: Arc<Mutex<InMemoryBlobProvider>>,
    /// Fallback online blob provider.
    /// This is used primarily during sync when archived blobs
    /// aren't provided by reth since they'll be too old.
    ///
    /// The `WithFallback` setup allows to specify two different
    /// endpoints for a primary and a fallback blob provider.
    online: OnlineBlobProviderWithFallback<
        OnlineBeaconClient,
        OnlineBeaconClient,
        SimpleSlotDerivation,
    >,
}

/// A blob provider that hold blobs in memory.
#[derive(Debug)]
pub struct InMemoryBlobProvider {
    /// Maps block hashes to blobs.
    blocks_to_blob: FIFOMap<B256, Vec<Blob>>,
}

impl InMemoryBlobProvider {
    /// Creates a new [InMemoryBlobProvider].
    pub fn with_capacity(cap: usize) -> Self {
        Self { blocks_to_blob: FIFOMap::with_capacity(cap) }
    }

    /// Inserts multiple blobs into the provider.
    #[allow(unused)]
    pub fn insert_blobs(&mut self, block_hash: B256, blobs: Vec<Blob>) {
        if let Some(existing_blobs) = self.blocks_to_blob.get_mut(&block_hash) {
            existing_blobs.extend(blobs);
        } else {
            self.blocks_to_blob.insert(block_hash, blobs);
        }
    }
}

impl ExExBlobProvider {
    /// Creates a new [ExExBlobProvider] with a local blob store, an online primary beacon
    /// client and an optional fallback blob archiver for fetching blobs.
    pub fn new(beacon_client_url: Url, blob_archiver_url: Option<Url>) -> Self {
        let memory = Arc::new(Mutex::new(InMemoryBlobProvider::with_capacity(256)));

        let online = OnlineBlobProviderBuilder::new()
            .with_primary(beacon_client_url.to_string())
            .with_fallback(blob_archiver_url.map(|url| url.to_string()))
            .build();

        Self { memory, online }
    }

    /// Inserts multiple blobs into the in-memory provider.
    pub fn insert_blobs(&mut self, block_hash: B256, blobs: Vec<Blob>) {
        self.memory.lock().insert_blobs(block_hash, blobs);
    }

    /// Attempts to fetch blobs using the in-memory blob store.
    async fn memory_blob_load(
        &mut self,
        block_ref: &BlockInfo,
        hashes: &[IndexedBlobHash],
    ) -> eyre::Result<Vec<Blob>> {
        let locked = self.memory.lock();

        let blobs_for_block = locked
            .blocks_to_blob
            .get(&block_ref.hash)
            .ok_or_else(|| eyre::eyre!("Blob not found for block ref: {:?}", block_ref))?;

        let mut blobs = Vec::new();
        for _ in hashes {
            for blob in blobs_for_block {
                blobs.push(*blob);
            }
        }

        Ok(blobs)
    }
}

#[async_trait]
impl BlobProvider for ExExBlobProvider {
    /// Fetches blobs for a given block ref and the blob hashes.
    async fn get_blobs(
        &mut self,
        block_ref: &BlockInfo,
        blob_hashes: &[IndexedBlobHash],
    ) -> Result<Vec<Blob>, BlobProviderError> {
        if let Ok(b) = self.memory_blob_load(block_ref, blob_hashes).await {
            return Ok(b);
        } else {
            warn!(target: "blob-provider", "Blob provider falling back to online provider");
            self.online.get_blobs(block_ref, blob_hashes).await
        }
    }
}
