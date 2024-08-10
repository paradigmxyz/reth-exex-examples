//! CLI Extension module for Kona

use clap::Args;
use reqwest::Url;
use reth::rpc::types::engine::JwtSecret;

pub const DEFAULT_L2_RPC_URL: &str = "https://optimism.llamarpc.com";

pub const DEFAULT_BEACON_CLIENT_URL: &str = "http://localhost:5052";

#[derive(Debug, Clone, Args)]
pub(crate) struct KonaArgsExt {
    /// RPC URL of an L2 node
    #[clap(long = "kona.l2-rpc-url", default_value = DEFAULT_L2_RPC_URL)]
    pub l2_rpc_url: Url,

    /// URL of the beacon client to fetch blobs
    #[clap(long = "kona.beacon-client-url", default_value = DEFAULT_BEACON_CLIENT_URL)]
    pub beacon_client_url: Url,

    /// The payload validation mode to use
    #[clap(
        long = "kona.validation-mode",
        default_value = "trusted",
        requires_if("engine-api", "kona.l2-engine-api-url"),
        requires_if("engine-api", "kona.l2-engine-jwt")
    )]
    pub validation_mode: ValidationMode,

    /// If the mode is "engine api", we also need an URL for it.
    #[clap(long = "kona.l2-engine-api-url")]
    pub l2_engine_api_url: Option<Url>,

    /// If the mode is "engine api", we also need a JWT for it.
    #[clap(long = "kona.l2-engine-jwt")]
    pub l2_engine_jwt: Option<JwtSecret>,
}

#[derive(Debug, Clone)]
pub(crate) enum ValidationMode {
    Trusted,
    EngineApi,
}

impl std::str::FromStr for ValidationMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "trusted" => Ok(ValidationMode::Trusted),
            "engine-api" => Ok(ValidationMode::EngineApi),
            _ => Err(format!("Invalid validation mode: {}", s)),
        }
    }
}
