use std::io;

use reth_tracing::tracing::info;
use wasi_common::{pipe::WritePipe, sync::WasiCtxBuilder, WasiCtx};
use wasmtime::{Engine, Linker, Memory, Module, Store, TypedFunc};

type AllocTypedFunc = TypedFunc<(i64,), i64>;
type NotificationTypedFunc = TypedFunc<(i64, i64), i64>;

pub struct RunningExEx {
    pub name: String,
    pub store: Store<WasiCtx>,
    pub memory: Memory,
    pub alloc_func: AllocTypedFunc,
    pub notification_func: NotificationTypedFunc,
}

impl RunningExEx {
    pub fn new(
        name: String,
        engine: &Engine,
        module: &Module,
        linker: &Linker<WasiCtx>,
    ) -> eyre::Result<Self> {
        let stdout = WritePipe::new(io::stdout());
        let wasi = WasiCtxBuilder::new().stdout(Box::new(stdout)).build();
        let mut store = Store::new(engine, wasi);

        let instance = linker
            .instantiate(&mut store, module)
            .map_err(|err| eyre::eyre!("failed to instantiate module: {err}"))?;
        let alloc_func = instance
            .get_typed_func::<(i64,), i64>(&mut store, "alloc")
            .map_err(|err| eyre::eyre!("failed to get alloc func: {err}"))?;
        let notification_func = instance
            .get_typed_func::<(i64, i64), i64>(&mut store, "notification")
            .map_err(|err| eyre::eyre!("failed to get notification func: {err}"))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| eyre::eyre!("failed to get memory"))?;

        Ok(Self { name, store, memory, alloc_func, notification_func })
    }

    pub fn handle_notification(
        &mut self,
        notification: &reth_exex::ExExNotification,
    ) -> eyre::Result<()> {
        let serialized_notification = serde_json::to_vec(&notification)?;
        let data_size = serialized_notification.len() as i64;
        let data_ptr = self
            .alloc_func
            .call(&mut self.store, (data_size,))
            .map_err(|err| eyre::eyre!("failed to call alloc func: {err}"))?;

        self.memory.write(&mut self.store, data_ptr as usize, &serialized_notification)?;

        let output = self
            .notification_func
            .call(&mut self.store, (data_ptr, data_size))
            .map_err(|err| eyre::eyre!("failed to call notification func: {err}"))?;

        info!(%self.name, ?data_ptr, ?data_size, ?output, "Called function");

        Ok(())
    }
}
