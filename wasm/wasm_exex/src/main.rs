#![no_main]

use core::slice;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use reth_tracing::{
    tracing::info, tracing_appender::non_blocking::WorkerGuard, RethTracer, Tracer,
};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

static TRACER: Lazy<Option<WorkerGuard>> = Lazy::new(|| RethTracer::new().init().unwrap());

/// Latest allocation made by the `alloc` function.
/// Used to check that the call to `process` is preceded by a call to `alloc`.
// TODO(alexey): we're single-threaded, use something easeir.
static LATEST_ALLOCATION: Mutex<Option<(i64, i64)>> = Mutex::new(None);

#[no_mangle]
pub extern "C" fn alloc(data_size: i64) -> i64 {
    let mut buf = Vec::with_capacity(data_size as usize);
    let ptr = buf.as_mut_ptr();
    // Prevent the buffer from being dropped.
    std::mem::forget(buf);
    let data_ptr = ptr as *const u8 as i64;

    *LATEST_ALLOCATION.lock().expect("failed to acquire mutex") = Some((ptr as i64, data_size));

    data_ptr
}

#[no_mangle]
pub extern "C" fn process(data_ptr: i64, data_size: i64) -> i64 {
    Lazy::force(&TRACER);

    // Check that the last allocation matches the passed arguments.
    assert_eq!(
        (data_ptr, data_size),
        LATEST_ALLOCATION.lock().expect("failed to acquire mutex").expect("no last allocation")
    );

    // SAFETY: the memory was allocated by the `alloc` and we check it above.
    let data = unsafe { slice::from_raw_parts(data_ptr as *const u8, data_size as usize) };

    // It's just a JSON for now, so let's print it as a string.
    let notification = String::from_utf8_lossy(data);
    if notification.len() > 200 {
        info!(
            notification =
                format!("{}...{}", &notification[..100], &notification[notification.len() - 100..]),
            "Received notification"
        );
    } else {
        info!(?notification, "Received notification");
    }

    notification.len() as i64
}
