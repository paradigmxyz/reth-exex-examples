use clap::Parser;
use cli_ext::OracleExt;
use exex::ExEx;
use network::{proto::OracleProtoHandler, Network};
use offchain_data::DataFeederStream;
use oracle::Oracle;
use reth::args::utils::DefaultChainSpecParser;
use reth_network::{protocol::IntoRlpxSubProtocol, NetworkProtocols};
use reth_node_ethereum::EthereumNode;

mod cli_ext;
mod exex;
mod network;
mod offchain_data;
mod oracle;

const ORACLE_EXEX_ID: &str = "exex-oracle";

fn main() -> eyre::Result<()> {
    reth::cli::Cli::<DefaultChainSpecParser, OracleExt>::parse().run(|builder, args| async move {
        let tcp_port = args.tcp_port;
        let udp_port = args.udp_port;

        let handle = builder
            .node(EthereumNode::default())
            .install_exex(ORACLE_EXEX_ID, move |ctx| async move {
                let subproto = OracleProtoHandler::new();
                ctx.network().add_rlpx_sub_protocol(subproto.into_rlpx_sub_protocol());

                let exex = ExEx::new(ctx);
                let network = Network::new(tcp_port, udp_port).await?;
                let data_feed = DataFeederStream::new(args.binance_symbols).await?;
                let oracle = Oracle::new(exex, network, data_feed);
                Ok(oracle)
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
