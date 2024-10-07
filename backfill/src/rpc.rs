use alloy_primitives::BlockNumber;
use async_trait::async_trait;
use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    types::{error::INTERNAL_ERROR_CODE, ErrorObject},
};
use tokio::sync::{mpsc, oneshot};

use crate::BackfillMessage;

#[rpc(server, namespace = "backfill")]
trait BackfillRpcExtApi {
    /// Starts backfilling the given range of blocks asynchronously.
    ///
    /// Returns the backfill job ID.
    #[method(name = "start")]
    async fn start(&self, from_block: BlockNumber, to_block: BlockNumber) -> RpcResult<u64>;

    /// Cancels the backfill job with the given ID.
    #[method(name = "cancel")]
    async fn cancel(&self, job_id: u64) -> RpcResult<()>;
}

/// The RPC module that exposes the backfill RPC methods and sends backfill messages to the ExEx.
pub struct BackfillRpcExt {
    /// Sender for backfill messages.
    pub backfill_tx: mpsc::UnboundedSender<BackfillMessage>,
}

impl BackfillRpcExt {
    async fn send_backfill_message<T>(
        &self,
        message: impl FnOnce(oneshot::Sender<eyre::Result<T>>) -> BackfillMessage,
    ) -> RpcResult<T> {
        let (tx, rx) = oneshot::channel();

        // Send the backfill message request to the ExEx
        self.backfill_tx.send(message(tx)).map_err(|err| {
            ErrorObject::owned(
                INTERNAL_ERROR_CODE,
                format!("failed to send backfill message request: {err:?}"),
                None::<()>,
            )
        })?;
        // Wait for the backfill message response in case any errors occurred
        rx.await
            .map_err(|err| {
                ErrorObject::owned(
                    INTERNAL_ERROR_CODE,
                    format!("failed to receive backfill message response: {err:?}"),
                    None::<()>,
                )
            })?
            .map_err(|err| ErrorObject::owned(INTERNAL_ERROR_CODE, err.to_string(), None::<()>))
    }
}

#[async_trait]
impl BackfillRpcExtApiServer for BackfillRpcExt {
    async fn start(&self, from_block: BlockNumber, to_block: BlockNumber) -> RpcResult<u64> {
        self.send_backfill_message(|response_tx| BackfillMessage::Start {
            range: from_block..=to_block,
            response_tx,
        })
        .await
    }

    async fn cancel(&self, job_id: u64) -> RpcResult<()> {
        self.send_backfill_message(|response_tx| BackfillMessage::Cancel { job_id, response_tx })
            .await
    }
}
