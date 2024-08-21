use std::{ffi::OsStr, path::PathBuf};

use clap::{Args, Parser};
use reth_exex::{ExExContext, ExExEvent};
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthereumNode;

async fn exex<Node: FullNodeComponents>(
    mut ctx: ExExContext<Node>,
    path: impl AsRef<OsStr>,
) -> eyre::Result<()> {
    let lib = unsafe { libloading::Library::new(path)? };
    let greet = unsafe { lib.get::<extern "C" fn()>(b"greet") }?;
    let process_notification = unsafe {
        lib.get::<extern "C" fn(&[u8]) -> eyre::Result<Option<ExExEvent>>>(b"process_notification")
    }?;

    greet();

    while let Some(notification) = ctx.notifications.recv().await {
        let data = serde_json::to_vec(&notification)?;
        let event = process_notification(&data)?;
        if let Some(event) = event {
            ctx.events.send(event)?;
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Args)]
struct ArgsExt {
    #[clap(long = "dylib-path")]
    pub dylib_path: PathBuf,
}

fn main() -> eyre::Result<()> {
    reth::cli::Cli::<ArgsExt>::parse().run(|builder, args| async move {
        let handle = builder
            .node(EthereumNode::default())
            .install_exex("Dylib", |ctx| async move { Ok(exex(ctx, args.dylib_path)) })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
