use clap::Parser;
use cli_ext::OracleExt;
use exex::ExEx;
use futures::{FutureExt, Stream};
use network::{proto::OracleProtoHandler, OracleNetwork};
use offchain_data::{DataFeederError, DataFeederStream, DataFeeds};
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
async fn start<Node: FullNodeComponents, D>(
    ctx: ExExContext<Node>,
    tcp_port: u16,
    udp_port: u16,
    data_feeder: D,
) -> eyre::Result<(Oracle<Node, D>, Node::Network)>
where
    Node::Network: NetworkProtocols,
    D: Stream<Item = Result<DataFeeds, DataFeederError>> + Send + 'static,
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

    // The oracle instance that will orchestrate the network, the execution extensions,
    // the off-chain data stream, and the gossiping
    let oracle = Oracle::new(exex, network, data_feeder, to_peers);
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
                        let data_feed = DataFeederStream::new(args.binance_symbols).await?;
                        let (oracle, _net) = start(ctx, tcp_port, udp_port, data_feed).await?;
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
    use reth_network::{NetworkEvent, NetworkEventListenerProvider, NetworkInfo, Peers};
    use reth_network_api::PeerId;
    use reth_tokio_util::EventStream;
    use reth_tracing::tracing::info;

    async fn wait_for_session(mut events: EventStream<NetworkEvent>) -> PeerId {
        while let Some(event) = events.next().await {
            if let NetworkEvent::SessionEstablished { peer_id, .. } = event {
                info!("Session established with {}", peer_id);
                return peer_id;
            }
            info!("Unexpected event: {:?}", event);
        }

        unreachable!()
    }

    fn mock_stream() -> impl StreamExt<
        Item = Result<crate::offchain_data::DataFeeds, crate::offchain_data::DataFeederError>,
    > {
        futures::stream::empty()
    }

    #[tokio::test]
    async fn oracles_can_peer() {
        reth_tracing::init_test_tracing();

        // spawn first instance
        let (ctx_1, _handle) = test_exex_context().await.unwrap();
        let data_feed1 = mock_stream();
        let (oracle_1, network_1) = start(ctx_1, 30303, 30304, data_feed1).await.unwrap();
        tokio::spawn(oracle_1);
        let net_1_events = network_1.event_listener();

        // spawn second instance
        let (ctx_2, _handle) = test_exex_context().await.unwrap();
        let data_feed2 = mock_stream();
        let (oracle_2, network_2) = start(ctx_2, 30305, 30306, data_feed2).await.unwrap();
        tokio::spawn(oracle_2);
        let net_2_events = network_2.event_listener();

        // expect peers connected
        let (peer_2, addr_2) = (network_2.peer_id(), network_2.local_addr());
        network_1.add_peer(*peer_2, addr_2);
        let expected_peer_2 = wait_for_session(net_1_events).await;
        assert_eq!(expected_peer_2, *peer_2);

        let (peer_1, addr_1) = (network_1.peer_id(), network_1.local_addr());
        network_2.add_peer(*peer_1, addr_1);
        let expected_peer_1 = wait_for_session(net_2_events).await;
        assert_eq!(expected_peer_1, *peer_1);
    }
}
