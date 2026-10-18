//! Playout reports (design §7.6, calls §5.3): `jitter_buffer_ms` is the growth of libwebrtc's
//! `jitterBufferDelay` (seconds, cumulative) over the growth of `jitterBufferEmittedCount` since
//! the previous report, the running mean for the first; a missing stat leaves its field out.

/// The previous report's counters, for the next one's growth.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Jitter {
    prev: Option<(f64, u64)>,
}

impl Jitter {
    /// The jitter buffer's delay in milliseconds since the previous report, from the cumulative
    /// `delay_s` and `emitted`; `None` when a stat is missing or nothing was emitted since.
    /// Counters that went back (a reset) count as the first report.
    pub fn report(&mut self, delay_s: Option<f64>, emitted: Option<u64>) -> Option<f64> {
        let (Some(d), Some(c)) = (delay_s.filter(|d| d.is_finite()), emitted) else {
            return None;
        };
        let out = match self.prev {
            Some((pd, pc)) if c > pc && d >= pd => Some((d - pd) / (c - pc) as f64 * 1000.0),
            Some((_, pc)) if c == pc => None,
            _ if c > 0 => Some(d / c as f64 * 1000.0),
            _ => None,
        };
        self.prev = Some((d, c));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_report_is_the_running_mean_then_the_growth() {
        let mut j = Jitter::default();
        // 4800 samples emitted with 288 s of summed delay: 60 ms each.
        assert_eq!(j.report(Some(288.0), Some(4800)), Some(60.0));
        // 960 more with 38.4 s more: 40 ms each.
        let r = j.report(Some(326.4), Some(5760)).unwrap();
        assert!((r - 40.0).abs() < 1e-9, "{r}");
        // Nothing emitted since: no field.
        assert_eq!(j.report(Some(326.4), Some(5760)), None);
        // A missing stat: no field, and the counters stay.
        assert_eq!(j.report(None, Some(6000)), None);
        let r = j.report(Some(336.0), Some(6000)).unwrap();
        assert!((r - 40.0).abs() < 1e-9, "{r}");
        // Counters that went back read as a first report.
        assert_eq!(j.report(Some(1.0), Some(20)), Some(50.0));
    }
}
