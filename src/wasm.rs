use std::{fs::File, path::Path};

use jsonrpsee::tracing::info;
use reth_tracing::tracing::debug;
use wasi_common::{pipe::WritePipe, sync::WasiCtxBuilder, WasiCtx};
use wasmtime::{Engine, Linker, Memory, Module, Store, TypedFunc};

type AllocParams = (i64,);
type AllocReturn = i64;
type NotificationParams = (i64, i64);
type NotificationReturn = i64;

pub struct RunningExEx {
    pub name: String,
    pub store: Store<WasiCtx>,
    pub memory: Memory,
    pub alloc_func: TypedFunc<AllocParams, AllocReturn>,
    pub process_func: TypedFunc<NotificationParams, NotificationReturn>,
}

impl RunningExEx {
    /// Creates a new instance of a running WASM-powered ExEx.
    ///
    /// Initializes a WASM instance with WASI support, prepares the memory and the typed
    /// functions.
    pub fn new(
        name: String,
        engine: &Engine,
        module: &Module,
        linker: &Linker<WasiCtx>,
        logs_directory: impl AsRef<Path>,
    ) -> eyre::Result<Self> {
        // TODO(alexey): ideally setup tracer with a span
        let file = File::create(logs_directory.as_ref().join(format!("{name}.log")))?;
        let wasi = WasiCtxBuilder::new().stdout(Box::new(WritePipe::new(file))).build();
        let mut store = Store::new(engine, wasi);

        let instance = linker
            .instantiate(&mut store, module)
            .map_err(|err| eyre::eyre!("failed to instantiate: {err}"))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| eyre::eyre!("failed to get memory"))?;
        let alloc_func = instance
            .get_typed_func::<AllocParams, AllocReturn>(&mut store, "alloc")
            .map_err(|err| eyre::eyre!("failed to get alloc func: {err}"))?;
        let process_func = instance
            .get_typed_func::<NotificationParams, NotificationReturn>(&mut store, "process")
            .map_err(|err| eyre::eyre!("failed to get process func: {err}"))?;

        Ok(Self { name, store, memory, alloc_func, process_func })
    }

    /// Processes an [`ExExNotification`] using the WASM instance.
    // TODO(alexey): we can probably use shared memory here to avoid copying the data into every
    // WASM instance memory. I tried it for a while and it didn't work straight away. Maybe we can
    // share a portion of linear memory, but the rest is up to the WASM instance to manage?
    pub fn process_notification(
        &mut self,
        notification: &reth_exex::ExExNotification,
    ) -> eyre::Result<()> {
        // TODO(alexey): serialize to bincode or just cast to bytes directly. Can't do it now
        // because `ExExNotification` can't be used inside WASM.
        let serialized_notification =
            // Can't even do JSON encode of a full struct smh, "key must be a string"
            serde_json::to_vec(&notification.committed_chain().map(|chain| chain.tip().header.clone()))?;

        // Allocate memory for the notification.
        let data_size = serialized_notification.len() as i64;
        let data_ptr = self
            .alloc_func
            .call(&mut self.store, (data_size,))
            .map_err(|err| eyre::eyre!("failed to call alloc func: {err}"))?;

        // // Write the notification to the allocated memory.
        self.memory.write(&mut self.store, data_ptr as usize, &serialized_notification)?;

        // // Call the notification function that will read the allocated memoyry.
        let output = self
            .process_func
            .call(&mut self.store, (data_ptr, data_size))
            .map_err(|err| eyre::eyre!("failed to call notification func: {err}"))?;

        info!(target: "wasm", name = %self.name, ?data_ptr, ?data_size, ?output, "Processed notification");

        info!("processed notification");

        Ok(())
    }
}
