use clap::Parser;
use cli_ext::OracleExt;
use exex::ExEx;
use network::{proto::OracleProtoHandler, OracleNetwork};
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
                // define the oracle subprotocol
                let (subproto, proto_events) = OracleProtoHandler::new();
                // add it to the network as a subprotocol
                ctx.network().add_rlpx_sub_protocol(subproto.into_rlpx_sub_protocol());

                // the instance of the execution extension that will handle chain events
                let exex = ExEx::new(ctx);

                // the instance of the oracle network that will handle discovery and
                // gossiping of data
                let (network, to_gossip) =
                    OracleNetwork::new(proto_events, tcp_port, udp_port).await?;
                // the off-chain data feed stream
                let data_feed = DataFeederStream::new(args.binance_symbols).await?;

                // the oracle instance that will orchestrate the network, the execution extensions,
                // the offchain data stream and the gossiping
                // the oracle will always sign and broadcast data via the channel until a peer is
                // subcribed to it
                let oracle = Oracle::new(exex, network, data_feed, to_gossip);
                Ok(oracle)
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
