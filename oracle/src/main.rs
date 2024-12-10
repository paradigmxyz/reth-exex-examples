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
    use crate::{offchain_data::binance::ticker::Ticker, start};
    use futures::{Stream, StreamExt};
    use reth_exex_test_utils::test_exex_context;
    use reth_network::{NetworkEvent, NetworkEventListenerProvider, NetworkInfo, Peers};
    use reth_network_api::PeerId;
    use reth_tracing::tracing::info;
    use tokio_stream::wrappers::BroadcastStream;

    async fn wait_for_session(mut events: impl Stream<Item = NetworkEvent> + Unpin) -> PeerId {
        while let Some(event) = events.next().await {
            if let NetworkEvent::ActivePeerSession { info, .. } = event {
                info!("Session established with {}", info.peer_id);
                return info.peer_id;
            }
            info!("Unexpected event: {:?}", event);
        }

        unreachable!()
    }

    use crate::offchain_data::{DataFeederError, DataFeeds};
    use futures::stream::{self};
    use std::pin::Pin;

    fn mock_stream() -> Pin<Box<dyn Stream<Item = Result<DataFeeds, DataFeederError>> + Send>> {
        let ticker = Ticker {
            event_type: "24hrTicker".to_string(),
            event_time: 1698323450000,
            symbol: "BTCUSDT".to_string(),
            price_change: "100.00".to_string(),
            price_change_percent: "2.5".to_string(),
            weighted_avg_price: "40200.00".to_string(),
            prev_close_price: "40000.00".to_string(),
            last_price: "40100.00".to_string(),
            last_quantity: "0.5".to_string(),
            best_bid_price: "40095.00".to_string(),
            best_bid_quantity: "1.0".to_string(),
            best_ask_price: "40105.00".to_string(),
            best_ask_quantity: "1.0".to_string(),
            open_price: "39900.00".to_string(),
            high_price: "40500.00".to_string(),
            low_price: "39800.00".to_string(),
            volume: "1500".to_string(),
            quote_volume: "60000000".to_string(),
            open_time: 1698237050000,
            close_time: 1698323450000,
            first_trade_id: 1,
            last_trade_id: 2000,
            num_trades: 2000,
        };

        // Wrap the Ticker in DataFeeds::Binance
        let data_feed = DataFeeds::Binance(ticker);

        // Create a stream that sends a single item and then ends, boxed and pinned
        Box::pin(stream::once(async { Ok(data_feed) }))
    }

    #[tokio::test]
    async fn e2e_oracles() {
        reth_tracing::init_test_tracing();

        // spawn first instance
        let (ctx_1, _handle) = test_exex_context().await.unwrap();
        let data_feed1 = mock_stream();
        let (oracle_1, network_1) = start(ctx_1, 30303, 30304, data_feed1).await.unwrap();
        let mut broadcast_stream_1 = BroadcastStream::new(oracle_1.signed_ticks().subscribe());
        let signer_1 = oracle_1.signer().address();
        tokio::spawn(oracle_1);
        let net_1_events = network_1.event_listener();

        // spawn second instance
        let (ctx_2, _handle) = test_exex_context().await.unwrap();
        let data_feed2 = mock_stream();
        let (oracle_2, network_2) = start(ctx_2, 30305, 30306, data_feed2).await.unwrap();
        let mut broadcast_stream_2 = BroadcastStream::new(oracle_2.signed_ticks().subscribe());
        let signer_2 = oracle_2.signer().address();
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

        // expect signed data
        let signed_ticker_1 = broadcast_stream_1.next().await.unwrap().unwrap();
        assert_eq!(signed_ticker_1.ticker.symbol, "BTCUSDT");
        assert_eq!(signed_ticker_1.signer, signer_1);

        let signed_ticker_2 = broadcast_stream_2.next().await.unwrap().unwrap();
        assert_eq!(signed_ticker_2.ticker.symbol, "BTCUSDT");
        assert_eq!(signed_ticker_2.signer, signer_2);
    }
}
