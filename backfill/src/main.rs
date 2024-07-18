use std::ops::RangeInclusive;

use async_trait::async_trait;
use futures::TryStreamExt;
use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    types::{error::INTERNAL_ERROR_CODE, ErrorObject},
};
use reth::primitives::{BlockNumber, Requests, SealedBlockWithSenders};
use reth_execution_types::ExecutionOutcome;
use reth_exex::{BackfillJobFactory, ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::{error, info};
use tokio::sync::mpsc;

struct BackfillExEx<Node: FullNodeComponents> {
    ctx: ExExContext<Node>,
    backfill_rx: mpsc::UnboundedReceiver<RangeInclusive<BlockNumber>>,

    backfill_job_factory: BackfillJobFactory<Node::Executor, Node::Provider>,
}

impl<Node: FullNodeComponents> BackfillExEx<Node> {
    fn new(
        ctx: ExExContext<Node>,
        backfill_rx: mpsc::UnboundedReceiver<RangeInclusive<BlockNumber>>,
    ) -> Self {
        let backfill_job_factory =
            BackfillJobFactory::new(ctx.block_executor().clone(), ctx.provider().clone());
        Self { ctx, backfill_rx, backfill_job_factory }
    }

    async fn start(mut self) -> eyre::Result<()> {
        loop {
            tokio::select! {
                Some(notification) = self.ctx.notifications.recv() => {
                    self.process_notification(notification).await?;
                }
                Some(range) = self.backfill_rx.recv() => {
                    let _ = self.backfill(range).await.inspect_err(|err| error!(%err, "Backfill error occurred"));
                }
            }
        }
    }

    async fn process_notification(&self, notification: ExExNotification) -> eyre::Result<()> {
        match &notification {
            ExExNotification::ChainCommitted { new } => {
                info!(committed_chain = ?new.range(), "Received commit");
            }
            ExExNotification::ChainReorged { old, new } => {
                info!(from_chain = ?old.range(), to_chain = ?new.range(), "Received reorg");
            }
            ExExNotification::ChainReverted { old } => {
                info!(reverted_chain = ?old.range(), "Received revert");
            }
        };

        if let Some(committed_chain) = notification.committed_chain() {
            self.process_committed_blocks(
                committed_chain.blocks().values(),
                committed_chain.execution_outcome(),
            )
            .await?;

            self.ctx.events.send(ExExEvent::FinishedHeight(committed_chain.tip().number))?;
        }

        Ok(())
    }

    async fn process_committed_blocks(
        &self,
        blocks: impl IntoIterator<Item = &SealedBlockWithSenders>,
        outcome: &ExecutionOutcome,
    ) -> eyre::Result<()> {
        let (blocks, transactions) =
            blocks.into_iter().fold((0, 0), |(mut blocks, mut transactions), block| {
                blocks += 1;
                transactions += block.body.len();
                (blocks, transactions)
            });
        info!(first_block = %outcome.first_block, %blocks, %transactions, "Processed committed blocks");
        Ok(())
    }

    async fn backfill(&self, range: RangeInclusive<BlockNumber>) -> eyre::Result<()> {
        self.backfill_job_factory
            .backfill(range)
            .into_stream()
            .map_err(Into::into)
            .try_for_each(|(block, output)| async {
                let sealed_block = block.seal_slow();
                let execution_outcome = ExecutionOutcome::new(
                    output.state,
                    output.receipts.into(),
                    sealed_block.number,
                    vec![Requests(output.requests)],
                );
                self.process_committed_blocks([&sealed_block], &execution_outcome).await
            })
            .await
    }
}

#[rpc(server, namespace = "backfill")]
trait BackfillRpcExtApi {
    #[method(name = "start")]
    async fn start(&self, from_block: BlockNumber, to_block: BlockNumber) -> RpcResult<()>;
}

struct BackfillRpcExt {
    backfill_tx: mpsc::UnboundedSender<RangeInclusive<BlockNumber>>,
}

#[async_trait]
impl BackfillRpcExtApiServer for BackfillRpcExt {
    async fn start(&self, from_block: BlockNumber, to_block: BlockNumber) -> RpcResult<()> {
        self.backfill_tx.send(from_block..=to_block).map_err(|err| {
            ErrorObject::owned(INTERNAL_ERROR_CODE, "internal error", Some(err.to_string()))
        })
    }
}

fn main() -> eyre::Result<()> {
    reth::cli::Cli::parse_args().run(|builder, _| async move {
        let (backfill_tx, backfill_rx) = mpsc::unbounded_channel();

        let handle = builder
            .node(EthereumNode::default())
            .extend_rpc_modules(move |ctx| {
                ctx.modules.merge_configured(BackfillRpcExt { backfill_tx }.into_rpc())?;
                Ok(())
            })
            .install_exex("Backfill", |ctx| async move {
                Ok(BackfillExEx::new(ctx, backfill_rx).start())
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
