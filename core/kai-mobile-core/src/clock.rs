//! The core's two clocks (design §12.2): elapsed real time for its deadlines, wall time for a
//! certificate's dates.
//!
//! - **Boot time** (`CLOCK_BOOTTIME`, Android's `elapsedRealtime`) keeps counting while the phone
//!   sleeps, unlike tokio's monotonic clock. The dead-link verdict and the next retry are kept on
//!   it; the core sleeps on tokio's timer as an approximation and compares with this clock at every
//!   wake (an incoming packet, the alarm Kotlin keeps for `Update::WakeAt`, a network callback, the
//!   app becoming visible), acting on what is due.
//! - **Wall time** (unix milliseconds) is what a certificate's `not_after` counts in.
//!
//! The tests use [`ManualClock`], whose boot time can jump as across a sleep.

use std::sync::{
    Arc,
    atomic::{AtomicI64, AtomicU64, Ordering},
};

/// Where the core reads the time.
pub trait Clock: Send + Sync + 'static {
    /// Milliseconds since boot, counting sleep.
    fn boot_ms(&self) -> u64;
    /// Unix milliseconds.
    fn wall_ms(&self) -> i64;
}

/// The system's clocks.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn boot_ms(&self) -> u64 {
        boottime_ms()
    }

    fn wall_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn boottime_ms() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, writable timespec; CLOCK_BOOTTIME exists on Linux and Android.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) };
    if rc != 0 {
        return monotonic_ms();
    }
    u64::try_from(ts.tv_sec).unwrap_or(0) * 1000
        + u64::try_from(ts.tv_nsec).unwrap_or(0) / 1_000_000
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn boottime_ms() -> u64 {
    monotonic_ms()
}

/// A monotonic stand-in (it stops while the machine sleeps), counted from the first call.
#[allow(dead_code)]
fn monotonic_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = *START.get_or_init(std::time::Instant::now);
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The system's clocks plus offsets a test moves: [`ManualClock::sleep`] jumps boot time (and wall
/// time with it) as a phone's sleep does, while tokio's timer does not move.
#[derive(Debug, Default)]
pub struct ManualClock {
    boot_offset: AtomicU64,
    wall_offset: AtomicI64,
}

impl ManualClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Boot time and wall time jump `ms` ahead.
    pub fn sleep(&self, ms: u64) {
        self.boot_offset.fetch_add(ms, Ordering::SeqCst);
        self.wall_offset
            .fetch_add(i64::try_from(ms).unwrap_or(i64::MAX), Ordering::SeqCst);
    }

    /// Wall time alone jumps `ms` (negative: back), as when the phone's clock is set.
    pub fn set_wall_offset(&self, ms: i64) {
        self.wall_offset.store(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn boot_ms(&self) -> u64 {
        SystemClock.boot_ms() + self.boot_offset.load(Ordering::SeqCst)
    }

    fn wall_ms(&self) -> i64 {
        SystemClock.wall_ms() + self.wall_offset.load(Ordering::SeqCst)
    }
}

/// A unix time in milliseconds as a UTC date (`2027-10-14`), as a certificate's lapse is said.
pub fn date(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_as_utc_days() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_823_522_591_000), "2027-10-14");
        assert_eq!(date(4_102_444_800_000), "2100-01-01");
    }

    #[test]
    fn a_manual_clock_jumps_boot_time_as_a_sleep() {
        let c = ManualClock::new();
        let before = c.boot_ms();
        c.sleep(3_600_000);
        assert!(c.boot_ms() >= before + 3_600_000);
        assert!(SystemClock.boot_ms() < c.boot_ms());
    }
}
