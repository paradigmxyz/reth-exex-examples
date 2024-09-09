use clap::Args;

pub const DEFAULT_DISCOVERY_PORT: u16 = 30304;
pub const DEFAULT_RLPX_PORT: u16 = 30303;
pub const DEFAULT_BINANCE_SYMBOLS: &str = "btcusdt,ethusdt";

#[derive(Debug, Clone, Args)]
pub(crate) struct OracleExt {
    /// TCP port used by RLPx
    #[clap(long = "disc.tcp-port", default_value_t = DEFAULT_RLPX_PORT)]
    pub tcp_port: u16,

    /// UDP port used for discovery
    #[clap(long = "disc.udp-port", default_value_t = DEFAULT_DISCOVERY_PORT)]
    pub udp_port: u16,

    /// The list of symbols to configure the stream of prices from binance.
    #[clap(long = "data.symbols", use_value_delimiter = true, default_value = DEFAULT_BINANCE_SYMBOLS)]
    pub binance_symbols: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Args, Parser};

    /// A helper type to parse Args more easily
    #[derive(Parser)]
    struct CommandParser<T: Args> {
        #[clap(flatten)]
        args: T,
    }

    #[test]
    fn test_oracle_ext() {
        let cli = CommandParser::<OracleExt>::parse_from(&[
            "test",
            "--disc.tcp-port",
            "30304",
            "--disc.udp-port",
            "30303",
            "--data.symbols",
            "btcusdt,ethusdt",
        ]);
        assert_eq!(cli.args.tcp_port, 30304);
        assert_eq!(cli.args.udp_port, 30303);
        assert_eq!(cli.args.binance_symbols, vec!["btcusdt", "ethusdt"]);
    }
}
