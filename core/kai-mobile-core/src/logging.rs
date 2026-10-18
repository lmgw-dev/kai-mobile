//! Logging (design §3.5): `tracing` to logcat, tag `kai`. Nothing leaves the phone. The pairing
//! link and its token, signatures and message contents are never logged; ids and codes are.
//! Off Android there is no subscriber of ours (the host tests install their own if they want one).

use std::sync::Once;

static INIT: Once = Once::new();

/// Installs the logcat subscriber once per process; later calls do nothing.
pub fn init() {
    INIT.call_once(install);
}

#[cfg(not(target_os = "android"))]
fn install() {}

#[cfg(target_os = "android")]
fn install() {
    use tracing_subscriber::layer::SubscriberExt;
    let subscriber = tracing_subscriber::registry().with(logcat::Layer);
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        // A subscriber is already set (a test harness or the host app): keep it.
    }
}

#[cfg(target_os = "android")]
mod logcat {
    use std::{ffi::CString, fmt::Write};

    use tracing::{Event, Level, Subscriber, field::Visit};
    use tracing_subscriber::{layer::Context, registry::LookupSpan};

    const TAG: &[u8] = b"kai\0";

    pub struct Layer;

    #[derive(Default)]
    struct Line(String);

    impl Visit for Line {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            if field.name() == "message" {
                let _ = write!(self.0, "{value:?}");
            } else {
                let _ = write!(self.0, "{}={value:?}", field.name());
            }
        }
    }

    impl<S: Subscriber + for<'a> LookupSpan<'a>> tracing_subscriber::Layer<S> for Layer {
        fn enabled(&self, metadata: &tracing::Metadata<'_>, _: Context<'_, S>) -> bool {
            // Debug builds log debug and up; release builds info and up.
            *metadata.level()
                <= if cfg!(debug_assertions) {
                    Level::DEBUG
                } else {
                    Level::INFO
                }
        }

        fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
            let mut line = Line::default();
            event.record(&mut line);
            let text = format!("{}: {}", event.metadata().target(), line.0);
            let priority = match *event.metadata().level() {
                Level::ERROR => android_log_sys::LogPriority::ERROR,
                Level::WARN => android_log_sys::LogPriority::WARN,
                Level::INFO => android_log_sys::LogPriority::INFO,
                Level::DEBUG => android_log_sys::LogPriority::DEBUG,
                Level::TRACE => android_log_sys::LogPriority::VERBOSE,
            };
            if let Ok(text) = CString::new(text.replace('\0', " ")) {
                // SAFETY: both strings are NUL-terminated and outlive the call.
                unsafe {
                    android_log_sys::__android_log_write(
                        priority as i32,
                        TAG.as_ptr().cast(),
                        text.as_ptr(),
                    );
                }
            }
        }
    }
}
