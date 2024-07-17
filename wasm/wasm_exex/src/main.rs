#![no_main]

use core::slice;

use once_cell::sync::Lazy;
use reth_tracing::{tracing::info, RethTracer, Tracer};
use tracing_appender::non_blocking::WorkerGuard;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

static TRACER: Lazy<Option<WorkerGuard>> = Lazy::new(|| RethTracer::new().init().unwrap());

#[no_mangle]
pub extern "C" fn alloc(size: i64) -> i64 {
    let mut buf = Vec::with_capacity(size as usize);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr as *const u8 as i64
}

#[no_mangle]
pub extern "C" fn notification(data_ptr: i64, data_size: i64) -> i64 {
    Lazy::force(&TRACER);

    let data = unsafe { slice::from_raw_parts(data_ptr as *const u8, data_size as usize) };
    let notification = String::from_utf8_lossy(data);
    if notification.len() > 50 {
        info!(
            notification =
                format!("{}...{}", &notification[..10], &notification[notification.len() - 10..]),
            "Received notification"
        );
    } else {
        info!(?notification, "Received notification");
    }

    notification.len() as i64
}
