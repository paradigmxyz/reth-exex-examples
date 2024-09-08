use clap::Parser;
use cli_ext::OracleExt;
use exex::ExEx;
use network::{proto::OracleProtoHandler, Network};
use oracle::Oracle;
use reth::args::utils::DefaultChainSpecParser;
use reth_network::{protocol::IntoRlpxSubProtocol, NetworkProtocols};
use reth_node_ethereum::EthereumNode;

mod cli_ext;
mod data_feeder;
mod exex;
mod network;
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
                let oracle = Oracle::new(exex, network).await?;
                Ok(oracle)
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
