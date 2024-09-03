use clap::Parser;
use cli_ext::OracleExt;
use disc::Discovery;
use exex::ExEx;
use reth_node_ethereum::EthereumNode;

mod cli_ext;
mod disc;
mod exex;

fn main() -> eyre::Result<()> {
    reth::cli::Cli::<OracleExt>::parse().run(|builder, args| async move {
        let tcp_port = args.tcp_port;
        let udp_port = args.udp_port;

        let handle = builder
            .node(EthereumNode::default())
            .install_exex("exex-oracle", move |ctx| async move {
                // start Discv5 task
                let disc = Discovery::new(tcp_port, udp_port).await?;

                // start exex task with discv5
                Ok(ExEx::new(ctx, disc))
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
