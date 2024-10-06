use clap::Parser;
use cli_ext::OracleExt;
use exex::ExEx;
use network::{proto::OracleProtoHandler, OracleNetwork};
use offchain_data::DataFeederStream;
use oracle::Oracle;
use reth::args::utils::DefaultChainSpecParser;
use reth_exex::ExExContext;
use reth_network::{protocol::IntoRlpxSubProtocol, NetworkProtocols};
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthereumNode;

mod cli_ext;
mod exex;
mod network;
mod offchain_data;
mod oracle;

const ORACLE_EXEX_ID: &str = "exex-oracle";

async fn start<Node: FullNodeComponents>(
    ctx: ExExContext<Node>,
    tcp_port: u16,
    udp_port: u16,
    binance_symbols: Vec<String>,
) -> eyre::Result<(Oracle<Node>, Node::Network)>
where
    Node::Network: NetworkProtocols,
{
    // Define the oracle subprotocol
    let (subproto, proto_events, to_peers) = OracleProtoHandler::new();
    // Add it to the network as a subprotocol
    let net = ctx.network().clone();
    net.add_rlpx_sub_protocol(subproto.into_rlpx_sub_protocol());

    // The instance of the execution extension that will handle chain events
    let exex = ExEx::new(ctx);

    // The instance of the oracle network that will handle discovery and gossiping of data
    let network = OracleNetwork::new(proto_events, tcp_port, udp_port).await?;
    // The off-chain data feed stream
    let data_feed = DataFeederStream::new(binance_symbols).await?;

    // The oracle instance that will orchestrate the network, the execution extensions,
    // the off-chain data stream, and the gossiping
    let oracle = Oracle::new(exex, network, data_feed, to_peers);
    Ok((oracle, net.clone()))
}

fn main() -> eyre::Result<()> {
    reth::cli::Cli::<DefaultChainSpecParser, OracleExt>::parse().run(|builder, args| async move {
        let tcp_port = args.tcp_port;
        let udp_port = args.udp_port;

        let handle = builder
            .node(EthereumNode::default())
            .install_exex(ORACLE_EXEX_ID, move |ctx| async move {
                // Define the oracle subprotocol
                let (subproto, proto_events, to_peers) = OracleProtoHandler::new();
                // Add it to the network as a subprotocol
                let net = ctx.network().clone();
                net.add_rlpx_sub_protocol(subproto.into_rlpx_sub_protocol());

                // The instance of the execution extension that will handle chain events
                let exex = ExEx::new(ctx);

                // The instance of the oracle network that will handle discovery and gossiping of
                // data
                let network = OracleNetwork::new(proto_events, tcp_port, udp_port).await?;
                // The off-chain data feed stream
                let data_feed = DataFeederStream::new(args.binance_symbols).await?;

                // The oracle instance that will orchestrate the network, the execution extensions,
                // the off-chain data stream, and the gossiping
                let oracle = Oracle::new(exex, network, data_feed, to_peers);
                Ok(oracle)
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}

#[cfg(test)]
mod tests {

    #[cfg(test)]
    mod tests {
        use crate::start;
        use reth_exex_test_utils::test_exex_context;

        #[tokio::test]
        async fn test_oracle() {
            reth_tracing::init_test_tracing();
            let (ctx, _handle) = test_exex_context().await.unwrap();
            let (oracle, network_1) =
                start(ctx, 30306, 30307, vec!["btcusdc".to_string(), "ethusdc".to_string()])
                    .await
                    .unwrap();
            tokio::spawn(oracle);

            // // spawn second
            // let (ctx, _handle) = test_exex_context().await.unwrap();
            // let (oracle, network_2) =
            //     start(ctx, 30308, 30309, vec!["btcusdc".to_string(), "ethusdc".to_string()])
            //         .await
            //         .unwrap();
            // tokio::spawn(oracle);

            // // make them connect
            // let (peer_1, addr_1) = (network_1.peer_id(), network_1.local_addr());
            // let (peer_2, addr_2) = (network_2.peer_id(), network_2.local_addr());
            // network_1.add_peer(*peer_1, addr_1);
            // network_2.add_peer(*peer_2, addr_2);

            // let mut events_1 = network_1.event_listener();

            // let expected_peer_2 = loop {
            //     if let Some(ev) = events_1.next().await {
            //         match ev {
            //             NetworkEvent::SessionEstablished { peer_id, .. } => {
            //                 info!("Session established with peer: {:?}", peer_id);
            //                 break peer_id;
            //             }
            //             _ => continue,
            //         }
            //     } else {
            //         unreachable!()
            //     }
            // };
            // assert_eq!(expected_peer_2, *peer_2);
        }
    }
}
