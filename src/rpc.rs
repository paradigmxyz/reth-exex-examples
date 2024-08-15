use std::fmt::Display;

use async_trait::async_trait;
use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    types::{error::INTERNAL_ERROR_CODE, ErrorObject, ErrorObjectOwned},
};
use tokio::sync::{mpsc, oneshot};

#[rpc(server, namespace = "exex")]
trait ExExRpcExtApi {
    #[method(name = "install")]
    async fn install(&self, name: String, wasm_base64: String) -> RpcResult<()>;

    #[method(name = "start")]
    async fn start(&self, name: String) -> RpcResult<()>;

    #[method(name = "stop")]
    async fn stop(&self, name: String) -> RpcResult<()>;
}

pub struct ExExRpcExt {
    pub to_exex: mpsc::UnboundedSender<(RpcMessage, oneshot::Sender<RpcResult<()>>)>,
}

#[derive(Debug)]
pub enum RpcMessage {
    Install(String, String),
    Start(String),
    Stop(String),
}

impl Display for RpcMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpcMessage::Install(name, bytecode) => {
                write!(f, "install {name} with bytecode of length {}", bytecode.len())
            }
            RpcMessage::Start(name) => write!(f, "start {name}"),
            RpcMessage::Stop(name) => write!(f, "stop {name}"),
        }
    }
}

#[async_trait]
impl ExExRpcExtApiServer for ExExRpcExt {
    async fn install(&self, name: String, wasm_base64: String) -> RpcResult<()> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .to_exex
            .send((RpcMessage::Install(name, wasm_base64), tx))
            .map_err(|_| rpc_internal_error())?;
        rx.await.map_err(|_| rpc_internal_error())?
    }

    async fn start(&self, name: String) -> RpcResult<()> {
        let (tx, rx) = oneshot::channel();
        let _ =
            self.to_exex.send((RpcMessage::Start(name), tx)).map_err(|_| rpc_internal_error())?;
        rx.await.map_err(|_| rpc_internal_error())?
    }

    async fn stop(&self, name: String) -> RpcResult<()> {
        let (tx, rx) = oneshot::channel();
        let _ =
            self.to_exex.send((RpcMessage::Stop(name), tx)).map_err(|_| rpc_internal_error())?;
        rx.await.map_err(|_| rpc_internal_error())?
    }
}

#[inline]
fn rpc_internal_error() -> ErrorObjectOwned {
    ErrorObject::owned(INTERNAL_ERROR_CODE, "internal error", Some(""))
}

macro_rules! rpc_internal_error_format {
    ($($arg:tt)*) => {
        jsonrpsee::types::error::ErrorObject::owned(jsonrpsee::types::error::INTERNAL_ERROR_CODE, format!($($arg)*), Some(""))
    };
}

pub(crate) use rpc_internal_error_format;
