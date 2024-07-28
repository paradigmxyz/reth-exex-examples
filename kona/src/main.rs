use std::sync::Arc;

use clap::Parser;
use eyre::{bail, Result};
use kona_derive::{
    online::{AlloyL2ChainProvider, EthereumDataSource},
    traits::{ChainProvider, L2ChainProvider, OriginProvider, Pipeline, StepResult},
    types::{L2BlockInfo, RollupConfig, StageError, OP_MAINNET_CONFIG},
};
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use tracing::{debug, error, info, warn};

mod blobs;
use blobs::ExExBlobProvider;

mod cli_ext;
use cli_ext::KonaArgsExt;

mod pipeline;
use pipeline::{new_local_pipeline, LocalAttributesBuilder, LocalPipeline};

mod providers;
use providers::LocalChainProvider;

mod validation;
use validation::{AttributesValidator, TrustedValidator};

#[derive(Debug)]
pub(crate) struct KonaExEx<Node: FullNodeComponents> {
    /// The rollup configuration
    cfg: Arc<RollupConfig>,
    /// The context of the Execution Extension
    ctx: ExExContext<Node>,
    /// The chain provider to follow the chain state based on ExEx events
    chain_provider: LocalChainProvider,
    /// The L2 chain provider to fetch L2 block information and optionally
    /// verify newly derived payloads against
    l2_provider: AlloyL2ChainProvider,
    /// The validator to verify newly derived payloads
    validator: Box<dyn AttributesValidator + Send>,
    /// The derivation pipeline
    pipeline: LocalPipeline,
    /// The current L2 block we are processing
    l2_block_cursor: L2BlockInfo,
    /// Whether the ExEx is synced to the L2 genesis
    is_synced_to_l2_genesis: bool,
    /// Whether we should advance the L2 block cursor
    should_advance_l2_block_cursor: bool,
    /// The number of derived payloads so far
    derived_payloads_count: u64,
}

impl<Node: FullNodeComponents> KonaExEx<Node> {
    /// Creates a new instance of the Kona Execution Extension.
    pub async fn new(ctx: ExExContext<Node>, args: KonaArgsExt) -> Self {
        // TODO: make chain id configurable
        let cfg = Arc::new(OP_MAINNET_CONFIG);
        let mut chain_provider = LocalChainProvider::new();
        chain_provider.insert_l2_genesis_block(cfg.genesis.l1);

        let mut l2_provider = AlloyL2ChainProvider::new_http(args.l2_rpc_url.clone(), cfg.clone());
        let blob_provider = ExExBlobProvider::new_from_beacon_client(args.beacon_client_url);
        let dap = EthereumDataSource::new(chain_provider.clone(), blob_provider, &cfg);

        let attributes =
            LocalAttributesBuilder::new(cfg.clone(), l2_provider.clone(), chain_provider.clone());

        // TODO: add engine api validator
        let validator = Box::new(TrustedValidator::new_http(
            args.l2_rpc_url,
            cfg.canyon_time.unwrap_or_default(),
        ));

        // TODO: use static genesis block info without fetching it
        let cursor = l2_provider.l2_block_info_by_number(cfg.genesis.l2.number).await.unwrap();
        let tip = chain_provider.block_info_by_number(cursor.l1_origin.number).await.unwrap();

        let pipeline = new_local_pipeline(
            cfg.clone(),
            chain_provider.clone(),
            l2_provider.clone(),
            dap,
            attributes,
            tip,
        );

        Self {
            cfg,
            ctx,
            validator,
            pipeline,
            chain_provider,
            l2_provider,
            l2_block_cursor: cursor,
            is_synced_to_l2_genesis: false,
            should_advance_l2_block_cursor: false,
            derived_payloads_count: 0,
        }
    }

    /// Steps the L2 derivation pipeline and validates the prepared attributes.
    async fn step_l2(&mut self) -> Result<()> {
        match self.pipeline.step(self.l2_block_cursor).await {
            StepResult::PreparedAttributes => info!(target: "loop", "Prepared attributes"),
            StepResult::AdvancedOrigin => info!(target: "loop", "Advanced origin"),
            StepResult::OriginAdvanceErr(err) => {
                warn!(target: "loop", ?err, "Error advancing origin")
            }
            StepResult::StepFailed(err) => match err {
                StageError::NotEnoughData => {
                    debug!(target: "loop", "Not enough data to step derivation pipeline");
                }
                _ => {
                    error!(target: "loop", ?err, "Error stepping derivation pipeline");
                }
            },
        }

        // Peek the the next prepared attributes and validate them
        if let Some(attributes) = self.pipeline.peek() {
            match self.validator.validate(attributes).await {
                Ok(true) => info!(target: "loop", "Attributes validated"),
                Ok(false) => {
                    warn!(target: "loop", "Attributes failed validation");
                    // If the validation fails, take the attributes out and continue
                    let _ = self.pipeline.next();
                    return Ok(());
                }
                Err(err) => {
                    error!(target: "loop", ?err, "Error validating attributes");
                    // If the attributes fail validation, retry them without taking them
                    // out of the pipeline
                    return Ok(());
                }
            }
        } else {
            debug!(target: "loop", "No prepared attributes to validate");
        };

        // Take the next attributes from the pipeline since they're valid.
        let attributes = if let Some(attributes) = self.pipeline.next() {
            attributes
        } else {
            error!(target: "loop", "Must have valid attributes");
            return Ok(());
        };

        // If we validated some attributes, we should advance the cursor.
        self.derived_payloads_count += 1;
        self.should_advance_l2_block_cursor = true;
        let derived_block = attributes.parent.block_info.number as i64 + 1;
        println!(
            "Validated Payload Attributes {} [L2 Block Num: {}] [L2 Timestamp: {}] [L1 Origin Block Num: {}]",
            self.derived_payloads_count,
            derived_block,
            attributes.attributes.timestamp,
            self.pipeline.origin().unwrap().number,
        );
        debug!(target: "loop", "attributes: {:#?}", attributes);

        Ok(())
    }

    fn handle_exex_notification(&mut self, notification: ExExNotification) -> Result<()> {
        if let Some(_reverted_chain) = notification.reverted_chain() {
            // TODO: handle reverted chain
        }
        if let Some(committed_chain) = notification.committed_chain() {
            let tip_number = committed_chain.tip().number; // TODO: ensure this is the right tip
            self.chain_provider.commit(committed_chain);
            if tip_number >= self.cfg.genesis.l1.number {
                tracing::debug!(target: "loop", "Chain synced to rollup genesis with L1 block number: {}", tip_number);
                self.is_synced_to_l2_genesis = true;
            }
            if let Err(err) = self.ctx.events.send(ExExEvent::FinishedHeight(tip_number)) {
                error!(target: "loop", ?err, "Failed to send ExEx event");
                bail!("Failed to send ExEx event: {:?}", err);
            }
        }

        Ok(())
    }

    /// Starts the Kona Execution Extension loop.
    pub async fn start(mut self) -> Result<()> {
        loop {
            if self.is_synced_to_l2_genesis {
                self.step_l2().await?;
            }

            if self.should_advance_l2_block_cursor {
                let next_l2_block = self.l2_block_cursor.block_info.number + 1;
                match self.l2_provider.l2_block_info_by_number(next_l2_block).await {
                    Ok(bi) => {
                        self.l2_block_cursor = bi;
                        self.should_advance_l2_block_cursor = false;
                    }
                    Err(err) => {
                        error!(target: "loop", ?err, "Failed to fetch next pending l2 safe head: {}", next_l2_block);
                        continue;
                    }
                }
            }

            if let Ok(notification) = self.ctx.notifications.try_recv() {
                self.handle_exex_notification(notification)?;
            }

            if self.ctx.notifications.is_closed() {
                warn!(target: "loop", "ExEx notification channel closed, exiting");
                bail!("ExEx notification channel closed");
            }
        }
    }
}

fn main() -> Result<()> {
    reth::cli::Cli::<cli_ext::KonaArgsExt>::parse().run(|builder, args| async move {
        let handle = builder
            .node(reth_node_ethereum::EthereumNode::default())
            .install_exex("Kona", move |ctx| async { Ok(KonaExEx::new(ctx, args).await.start()) })
            .launch()
            .await?;
        handle.wait_for_node_exit().await
    })
}
