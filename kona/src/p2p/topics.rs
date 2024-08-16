//! Topics for P2P 

/// Blocks Topics
pub enum BlocksTopic {
    V1(BlocksTopicV1),
    V2(BlocksTopicV2),
    V3(BlocksTopicV3),
}

/// Blocks Topics V1
pub struct BlocksTopicV1(u64);

impl BlocksTopicV1 {
    /// Creates a new [BlocksTopicV1] with the given chain id.
    pub fn new(chain_id: u64) -> Self {
        Self(chain_id)
    }
}

/// Converts a [u64] chain id into a [BlocksTopicV1]
impl From<u64> for BlocksTopicV1 {
    fn from(chain_id: u64) -> Self {
        Self(chain_id)
    }
}

impl std::fmt::Display for BlocksTopicV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "/optimism/{}/0/blocks", self.0)
    }
}

/// Blocks Topics V2
pub struct BlocksTopicV2(u64);

impl BlocksTopicV2 {
    /// Creates a new [BlocksTopicV2] with the given chain id.
    pub fn new(chain_id: u64) -> Self {
        Self(chain_id)
    }
}

/// Converts a [u64] chain id into a [BlocksTopicV2]
impl From<u64> for BlocksTopicV2 {
    fn from(chain_id: u64) -> Self {
        Self(chain_id)
    }
}

impl std::fmt::Display for BlocksTopicV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "/optimism/{}/1/blocks", self.0)
    }
}

/// Blocks Topics V3
pub struct BlocksTopicV3(u64);

impl BlocksTopicV3 {
    /// Creates a new [BlocksTopicV3] with the given chain id.
    pub fn new(chain_id: u64) -> Self {
        Self(chain_id)
    }
}

/// Converts a [u64] chain id into a [BlocksTopicV3]
impl From<u64> for BlocksTopicV3 {
    fn from(chain_id: u64) -> Self {
        Self(chain_id)
    }
}

impl std::fmt::Display for BlocksTopicV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "/optimism/{}/2/blocks", self.0)
    }
}
