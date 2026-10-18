//! The spike's one tokio runtime: multi-threaded with the default worker count, as the core's
//! will be (design §3.1), created at first use and kept for the process's life.

use std::sync::OnceLock;

use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn get() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .thread_name("spike-tokio")
            .enable_time()
            .build()
            .expect("the spike's tokio runtime")
    })
}

/// The worker count tokio chose (the number of cores).
pub fn workers() -> u32 {
    u32::try_from(get().metrics().num_workers()).unwrap_or(u32::MAX)
}
