use std::{ops::RangeInclusive, sync::Arc};

use async_trait::async_trait;
use futures::TryStreamExt;
use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    types::{error::INTERNAL_ERROR_CODE, ErrorObject},
};
use reth::primitives::{BlockNumber, Requests};
use reth_execution_types::{Chain, ExecutionOutcome};
use reth_exex::{BackfillJobFactory, ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::{error, info};
use tokio::sync::{mpsc, oneshot, Semaphore};

type BackfillRequest = (RangeInclusive<BlockNumber>, oneshot::Sender<eyre::Result<()>>);

/// The ExEx that consumes new [`ExExNotification`]s and processes new backfill requests by
/// [`BackfillRpcExt`].
struct BackfillExEx<Node: FullNodeComponents> {
    /// The context of the ExEx.
    ctx: ExExContext<Node>,
    /// Receiver for backfill requests and senders of responses.
    backfill_rx: mpsc::UnboundedReceiver<BackfillRequest>,
    /// Factory for backfill jobs.
    backfill_job_factory: BackfillJobFactory<Node::Executor, Node::Provider>,
    /// Semaphore to limit the number of concurrent backfills.
    backfill_semaphore: Arc<Semaphore>,
}

impl<Node: FullNodeComponents> BackfillExEx<Node> {
    /// Creates a new instance of the ExEx.
    fn new(
        ctx: ExExContext<Node>,
        backfill_rx: mpsc::UnboundedReceiver<BackfillRequest>,
        backfill_limit: usize,
    ) -> Self {
        let backfill_job_factory =
            BackfillJobFactory::new(ctx.block_executor().clone(), ctx.provider().clone());
        Self {
            ctx,
            backfill_rx,
            backfill_job_factory,
            backfill_semaphore: Arc::new(Semaphore::new(backfill_limit)),
        }
    }

    /// Starts listening for notifications and backfill requests.
    async fn start(mut self) -> eyre::Result<()> {
        loop {
            tokio::select! {
                Some(notification) = self.ctx.notifications.recv() => {
                    self.process_notification(notification).await?;
                }
                Some((range, backfill_tx)) = self.backfill_rx.recv() => {
                    let _ = backfill_tx
                        .send(self.backfill(range))
                        .inspect_err(|_| error!("Failed to send backfill response to RPC"));
                }
            }
        }
    }

    /// Processes the given notification and calls [`Self::process_committed_chain`] for every
    /// committed chain.
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
            Self::process_committed_chain(&committed_chain).await?;

            self.ctx.events.send(ExExEvent::FinishedHeight(committed_chain.tip().number))?;
        }

        Ok(())
    }

    /// Processes the committed chain and logs the number of blocks and transactions.
    pub async fn process_committed_chain(chain: &Chain) -> eyre::Result<()> {
        // Calculate the number of blocks and transactions in the committed chain
        let blocks = chain.blocks().len();
        let transactions = chain.blocks().values().map(|block| block.body.len()).sum::<usize>();

        info!(first_block = %chain.execution_outcome().first_block, %blocks, %transactions, "Processed committed blocks");
        Ok(())
    }

    /// Backfills the given range of blocks in parallel, calling the
    /// [`Self::process_committed_chain`] method for each block. Requires acquiring a semaphore
    /// permit that limits the number of concurrent backfills. The backfill job is spawned in a
    /// separate task.
    ///
    /// Returns an error if the semaphore permit could not be acquired.
    fn backfill(&self, range: RangeInclusive<BlockNumber>) -> eyre::Result<()> {
        let permit = self
            .backfill_semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|err| eyre::eyre!("concurrent backfills limit reached: {err:?}"))?;

        let job = self
            .backfill_job_factory
            // Create a backfill job for the given range
            .backfill(range);
        self.ctx.task_executor().spawn(async {
            let result = job
                // Convert the backfill job into a parallel stream
                .into_stream()
                // Covert the block execution error into `eyre`
                .map_err(Into::into)
                // Process each block, returning early if an error occurs
                .try_for_each(|(block, output)| async {
                    let sealed_block = block.seal_slow();
                    let execution_outcome = ExecutionOutcome::new(
                        output.state,
                        output.receipts.into(),
                        sealed_block.number,
                        vec![Requests(output.requests)],
                    );
                    let chain = Chain::new([sealed_block], execution_outcome, None);

                    // Process the committed blocks
                    Self::process_committed_chain(&chain).await
                })
                .await;

            if let Err(err) = result {
                error!(%err, "Backfill error occurred");
            }

            drop(permit);
        });

        Ok(())
    }
}

#[rpc(server, namespace = "backfill")]
trait BackfillRpcExtApi {
    /// Starts backfilling the given range of blocks asynchronously.
    #[method(name = "start")]
    async fn start(&self, from_block: BlockNumber, to_block: BlockNumber) -> RpcResult<()>;
}

/// The RPC module that exposes the backfill RPC methods and sends backfill requests to
/// [`BackfillExEx`].
struct BackfillRpcExt {
    /// Sender for backfill requests and receivers of responses.
    backfill_tx: mpsc::UnboundedSender<BackfillRequest>,
}

#[async_trait]
impl BackfillRpcExtApiServer for BackfillRpcExt {
    async fn start(&self, from_block: BlockNumber, to_block: BlockNumber) -> RpcResult<()> {
        let (tx, rx) = oneshot::channel();

        // Send the backfill request to the ExEx
        self.backfill_tx.send((from_block..=to_block, tx)).map_err(|err| {
            ErrorObject::owned(
                INTERNAL_ERROR_CODE,
                format!("failed to send backfill request: {err:?}"),
                None::<()>,
            )
        })?;
        // Wait for the backfill response in case any errors occurred
        rx.await
            .map_err(|err| {
                ErrorObject::owned(
                    INTERNAL_ERROR_CODE,
                    format!("failed to receive backfill response: {err:?}"),
                    None::<()>,
                )
            })?
            .map_err(|err| ErrorObject::owned(INTERNAL_ERROR_CODE, err.to_string(), None::<()>))
    }
}

fn main() -> eyre::Result<()> {
    reth::cli::Cli::parse_args().run(|builder, _| async move {
        // Create a channel for backfill requests. Sender will go to the RPC server, receiver will
        // be used by the ExEx.
        let (backfill_tx, backfill_rx) = mpsc::unbounded_channel();

        let handle = builder
            .node(EthereumNode::default())
            // Extend the RPC server with the backfill RPC module.
            .extend_rpc_modules(move |ctx| {
                ctx.modules.merge_configured(BackfillRpcExt { backfill_tx }.into_rpc())?;
                Ok(())
            })
            // Install the backfill ExEx.
            .install_exex("Backfill", |ctx| async move {
                Ok(BackfillExEx::new(ctx, backfill_rx, 10).start())
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
