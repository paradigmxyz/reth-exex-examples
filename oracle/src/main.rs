use clap::Parser;
use cli_ext::OracleExt;
use exex::ExEx;
use futures::FutureExt;
use network::{proto::OracleProtoHandler, OracleNetwork};
use offchain_data::DataFeederStream;
use oracle::Oracle;
use reth::chainspec::EthereumChainSpecParser;
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

/// Helper function to start the oracle stack.
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
    reth::cli::Cli::<EthereumChainSpecParser, OracleExt>::parse().run(|builder, args| async move {
        let tcp_port = args.tcp_port;
        let udp_port = args.udp_port;

        let handle = builder
            .node(EthereumNode::default())
            .install_exex(ORACLE_EXEX_ID, move |ctx| {
                // Rust seems to trigger a bogus higher-ranked lifetime error when using
                // just an async closure here -- using `spawn_blocking` avoids this
                // particular issue.
                //
                // To avoid the higher ranked lifetime error we use `spawn_blocking`
                // which will move the closure to another blocking-allowed thread,
                // then execute.
                //
                // Source: https://github.com/vados-cosmonic/wasmCloud/commit/440e8c377f6b02f45eacb02692e4d2fabd53a0ec
                tokio::task::spawn_blocking(move || {
                    tokio::runtime::Handle::current().block_on(async move {
                        let (oracle, _net) =
                            start(ctx, tcp_port, udp_port, args.binance_symbols).await?;
                        Ok(oracle)
                    })
                })
                .map(|result| result.map_err(Into::into).and_then(|result| result))
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}

#[cfg(test)]
mod tests {
    use crate::start;
    use futures::StreamExt;
    use reth_exex_test_utils::test_exex_context;
    use reth_network::{
        NetworkEvent, NetworkEventListenerProvider, NetworkHandle, NetworkInfo, Peers,
    };
    use reth_network_api::PeerId;
    use reth_tracing::tracing::info;

    async fn wait_for_session(network: &NetworkHandle) -> PeerId {
        let mut events = network.event_listener();

        while let Some(event) = events.next().await {
            if let NetworkEvent::SessionEstablished { peer_id, .. } = event {
                info!("Session established with {}", peer_id);
                return peer_id;
            }
            //info!("Unexpected event: {:?}", event);
        }

        unreachable!()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn can_peer_oracles() {
        reth_tracing::init_test_tracing();

        // spawn first instance
        let (ctx_1, _handle) = test_exex_context().await.unwrap();
        let (oracle_1, network_1) =
            start(ctx_1, 30303, 30304, vec!["btcusdc".to_string(), "ethusdc".to_string()])
                .await
                .unwrap();
        tokio::spawn(oracle_1);

        // spawn second instance
        let (ctx_2, _handle) = test_exex_context().await.unwrap();
        let (oracle_2, network_2) =
            start(ctx_2, 30305, 30306, vec!["btcusdc".to_string(), "ethusdc".to_string()])
                .await
                .unwrap();
        tokio::spawn(oracle_2);

        // make them connect
        let (peer_1, addr_1) = (network_1.peer_id(), network_1.local_addr());
        let (peer_2, addr_2) = (network_2.peer_id(), network_2.local_addr());

        network_1.add_peer(*peer_2, addr_2);
        let expected_peer_2 = wait_for_session(&network_1).await;
        info!("Peer1 connected to peer2: {:?}", expected_peer_2);
        assert_eq!(expected_peer_2, *peer_2);

        network_2.add_peer(*peer_1, addr_1);
        let expected_peer_1 = wait_for_session(&network_2).await;
        info!("Peer2 connected to peer1: {:?}", expected_peer_1);
        assert_eq!(expected_peer_1, *peer_1);
    }
}
