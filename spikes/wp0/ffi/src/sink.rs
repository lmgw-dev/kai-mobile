//! An `EventSink` call (design §3.2) from three kinds of thread. JNA attaches a native thread to
//! the JVM for a callback and, unless told otherwise, detaches it on return, so a call from a
//! tokio worker pays an attach and a detach each time; a call on the thread Kotlin called in on
//! (already attached) pays neither. The difference between the two is the attach-and-detach cost.

use std::{
    sync::{Arc, Mutex},
    thread::ThreadId,
    time::{Duration, Instant},
};

use crate::{Timings, runtime};

/// Kotlin's sink for the call's events: one oai-events message (or part) per call.
#[uniffi::export(callback_interface)]
pub trait EventSink: Send + Sync {
    fn send(&self, text: String);
}

/// What [`sink_bench`] measured, each a set of single calls in sequence (never concurrent).
#[derive(Debug, Clone, uniffi::Record)]
pub struct SinkReport {
    /// On the thread that called `sink_bench` (a Kotlin thread: already attached).
    pub calling_thread: Timings,
    /// From tokio worker threads, the first call on each worker.
    pub worker_first_call: Timings,
    /// From tokio worker threads, every later call on a worker that called before.
    pub worker_repeat_call: Timings,
    /// From a new native thread per call (spawned outside the timing).
    pub fresh_thread: Timings,
    /// How many distinct workers made calls.
    pub workers_used: u32,
    /// The payload's size in bytes, an oai-events message of typical length.
    pub payload_bytes: u32,
}

/// A typical oai-events message: `response.output_audio_transcript.delta` with a few words.
fn payload(i: u32) -> String {
    format!(
        r#"{{"type":"response.output_audio_transcript.delta","event_id":"event_{i:08}","response_id":"resp_0123456789abcdef","item_id":"item_0123456789abcdef","output_index":0,"content_index":0,"delta":"and that is why the sky looks blue "}}"#
    )
}

/// `calls` calls of each kind (fresh threads: at most 100), timed around the call alone.
#[uniffi::export]
pub fn sink_bench(sink: Box<dyn EventSink>, calls: u32) -> SinkReport {
    let sink: Arc<dyn EventSink> = Arc::from(sink);
    let calls = calls.max(1);
    let payload_bytes = u32::try_from(payload(0).len()).unwrap_or(u32::MAX);

    let mut here = Vec::with_capacity(calls as usize);
    for i in 0..calls {
        let text = payload(i);
        let t = Instant::now();
        sink.send(text);
        here.push(t.elapsed());
    }

    let rt = runtime::get();
    let seen: Arc<Mutex<Vec<ThreadId>>> = Arc::default();
    let mut first = Vec::new();
    let mut repeat = Vec::new();
    for i in 0..calls {
        let sink = sink.clone();
        let seen = seen.clone();
        let (took, was_first) = rt.block_on(async move {
            tokio::spawn(async move {
                let me = std::thread::current().id();
                let was_first = {
                    let mut seen = seen.lock().unwrap_or_else(|p| p.into_inner());
                    let new = !seen.contains(&me);
                    if new {
                        seen.push(me);
                    }
                    new
                };
                let text = payload(i);
                let t = Instant::now();
                sink.send(text);
                (t.elapsed(), was_first)
            })
            .await
            .unwrap_or((Duration::ZERO, false))
        });
        if was_first { &mut first } else { &mut repeat }.push(took);
    }
    let workers_used = u32::try_from(seen.lock().map_or(0, |s| s.len())).unwrap_or(u32::MAX);

    let mut fresh = Vec::new();
    for i in 0..calls.min(100) {
        let sink = sink.clone();
        let took = std::thread::spawn(move || {
            let text = payload(i);
            let t = Instant::now();
            sink.send(text);
            t.elapsed()
        })
        .join()
        .unwrap_or(Duration::ZERO);
        fresh.push(took);
    }

    SinkReport {
        calling_thread: Timings::of(&here),
        worker_first_call: Timings::of(&first),
        worker_repeat_call: Timings::of(&repeat),
        fresh_thread: Timings::of(&fresh),
        workers_used,
        payload_bytes,
    }
}
