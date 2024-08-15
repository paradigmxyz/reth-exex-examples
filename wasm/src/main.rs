mod rpc;
mod wasm;

use std::{collections::HashMap, path::PathBuf};

use jsonrpsee::core::RpcResult;
use reth::dirs::{LogsDir, PlatformPath};
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::{error, info};
use rpc::{rpc_internal_error_format, ExExRpcExt, ExExRpcExtApiServer, RpcMessage};
use tokio::sync::{mpsc, oneshot};
use wasi_common::WasiCtx;
use wasm::RunningExEx;
use wasmtime::{Engine, Linker, Module};

struct WasmExEx<Node: FullNodeComponents> {
    ctx: ExExContext<Node>,
    rpc_messages: mpsc::UnboundedReceiver<(RpcMessage, oneshot::Sender<RpcResult<()>>)>,
    logs_directory: PathBuf,

    engine: Engine,
    linker: Linker<WasiCtx>,

    installed_exexes: HashMap<String, Module>,
    running_exexes: HashMap<String, RunningExEx>,
}

impl<Node: FullNodeComponents> WasmExEx<Node> {
    fn new(
        ctx: ExExContext<Node>,
        rpc_messages: mpsc::UnboundedReceiver<(RpcMessage, oneshot::Sender<RpcResult<()>>)>,
        logs_directory: PathBuf,
    ) -> eyre::Result<Self> {
        let engine = Engine::default();
        let mut linker = Linker::<WasiCtx>::new(&engine);
        wasi_common::sync::add_to_linker(&mut linker, |s| s)
            .map_err(|err| eyre::eyre!("failed to add WASI: {err}"))?;

        Ok(Self {
            ctx,
            rpc_messages,
            logs_directory,
            engine,
            linker,
            installed_exexes: HashMap::new(),
            running_exexes: HashMap::new(),
        })
    }

    async fn start(mut self) -> eyre::Result<()> {
        loop {
            tokio::select! {
            Some(notification) = self.ctx.notifications.recv() => {
                    self.handle_notification(notification).await?
            }
            Some((rpc_message, tx)) = self.rpc_messages.recv() => {
                let _ = tx
                    .send(self.handle_rpc_message(rpc_message).await)
                    .inspect_err(|err| error!("failed to send response: {err:?}"));
                },
            }
        }
    }

    async fn handle_notification(&mut self, notification: ExExNotification) -> eyre::Result<()> {
        let committed_chain_tip = notification.committed_chain().map(|chain| chain.tip().number);

        for exex in self.running_exexes.values_mut() {
            if let Err(err) = exex.process_notification(&notification) {
                error!(name = %exex.name, %err, "failed to process notification")
            }
        }

        if let Some(tip) = committed_chain_tip {
            self.ctx.events.send(ExExEvent::FinishedHeight(tip))?;
        }

        info!(?committed_chain_tip, "Handled notification");

        Ok(())
    }

    async fn handle_rpc_message(&mut self, rpc_message: RpcMessage) -> RpcResult<()> {
        match &rpc_message {
            RpcMessage::Install(name, bytecode) => {
                let module = Module::new(&self.engine, bytecode).map_err(|err| {
                    rpc_internal_error_format!("failed to create module for {name}: {err}")
                })?;
                self.installed_exexes.insert(name.clone(), module);
            }
            RpcMessage::Start(name) => {
                let module = self
                    .installed_exexes
                    .get(name)
                    .ok_or_else(|| rpc_internal_error_format!("ExEx {name} not installed"))?;

                let exex = RunningExEx::new(
                    name.clone(),
                    &self.engine,
                    module,
                    &self.linker,
                    &self.logs_directory,
                )
                .map_err(|err| {
                    rpc_internal_error_format!("failed to create exex for {name}: {err}")
                })?;

                self.running_exexes.insert(name.clone(), exex);
            }
            RpcMessage::Stop(name) => {
                self.running_exexes.remove(name).ok_or_else(|| {
                    rpc_internal_error_format!("no running exex found for {name}")
                })?;
            }
        }

        info!(%rpc_message, "Handled RPC message");

        Ok(())
    }
}

fn main() -> eyre::Result<()> {
    reth::cli::Cli::parse_args().run(|builder, _| async move {
        let (rpc_tx, rpc_rx) = mpsc::unbounded_channel();

        let handle = builder
            .node(EthereumNode::default())
            .extend_rpc_modules(move |ctx| {
                ctx.modules.merge_configured(ExExRpcExt { to_exex: rpc_tx }.into_rpc())?;
                Ok(())
            })
            .install_exex("Minimal", |ctx| async move {
                // TODO(alexey): obviously bad but we don't have access to log args in the context
                let logs_directory = PlatformPath::<LogsDir>::default()
                    .with_chain(ctx.config.chain.chain, ctx.config.datadir.clone())
                    .as_ref()
                    .to_path_buf();
                Ok(WasmExEx::new(ctx, rpc_rx, logs_directory)?.start())
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
