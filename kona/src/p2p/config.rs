//! P2P Configuration

use std::time::Duration;
use lazy_static::lazy_static;
use libp2p::gossipsub::ConfigBuilder;

////////////////////////////////////////////////////////////////////////////////////////////////
// GossipSub Constants
////////////////////////////////////////////////////////////////////////////////////////////////

/// The maximum gossip size.
/// Limits the total size of gossip RPC containers as well as decompressed individual messages.
pub const MAX_GOSSIP_SIZE: usize = 10 * (1 << 20);

/// The minimum gossip size.
/// Used to make sure that there is at least some data to validate the signature against.
pub const MIN_GOSSIP_SIZE: usize = 66;

/// The maximum outbound queue.
pub const MAX_OUTBOUND_QUEUE: usize = 256;

/// The maximum validate queue.
pub const MAX_VALIDATE_QUEUE: usize = 256;

/// The global validate throttle.
pub const GLOBAL_VALIDATE_THROTTLE: usize = 512;

/// The default mesh D.
pub const DEFAULT_MESH_D: usize = 8;

/// The default mesh D low.
pub const DEFAULT_MESH_DLO: usize = 6;

/// The default mesh D high.
pub const DEFAULT_MESH_DHI: usize = 12;

/// The default mesh D lazy.
pub const DEFAULT_MESH_DLAZY: usize = 6;

////////////////////////////////////////////////////////////////////////////////////////////////
// Duration Constants
////////////////////////////////////////////////////////////////////////////////////////////////

lazy_static! {
    /// The gossip heartbeat.
    pub static ref GOSSIP_HEARTBEAT: Duration = Duration::from_millis(500);

    /// The seen messages TTL.
    /// Limits the duration that message IDs are remembered for gossip deduplication purposes.
    pub static ref SEEN_MESSAGES_TTL: Duration = 130 * *GOSSIP_HEARTBEAT;
    
    /// The pper score inspect frequency.
    /// The frequency at which peer scores are inspected.
    pub static ref PEER_SCORE_INSPECT_FREQUENCY: Duration = 15 * Duration::from_secs(1);
}

////////////////////////////////////////////////////////////////////////////////////////////////
// Config Building
////////////////////////////////////////////////////////////////////////////////////////////////

/// Builds the default gossipsub configuration.
///
/// Notable defaults:
/// - validation_mode: Strict
///
/// # Returns
///
/// A [`ConfigBuilder`] with the default gossipsub configuration already set.
/// Call `.build()` on the returned builder to get the final [libp2p::gossipsub::Config].
pub fn default_config_builder() -> ConfigBuilder {
    let mut builder = ConfigBuilder::default();
    builder
        .mesh_n(DEFAULT_MESH_D)
        .mesh_n_low(DEFAULT_MESH_DLO)
        .mesh_n_high(DEFAULT_MESH_DHI)
        .gossip_lazy(DEFAULT_MESH_DLAZY)
        .heartbeat_interval(*GOSSIP_HEARTBEAT)
        .fanout_ttl(Duration::from_secs(24))
        .history_length(12)
        .history_gossip(3);
    builder
}
