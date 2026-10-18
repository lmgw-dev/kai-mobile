//! A set of durations as the spike reports them: microseconds, the first sample kept apart.

use std::time::Duration;

/// Durations in microseconds. `first_us` is the first sample as taken (often the cold one); the
/// order statistics cover every sample, the first included.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Timings {
    pub count: u32,
    pub first_us: f64,
    pub min_us: f64,
    pub median_us: f64,
    pub p90_us: f64,
    pub max_us: f64,
    pub mean_us: f64,
}

impl Timings {
    /// The statistics of `samples`; all zero when there are none.
    pub fn of(samples: &[Duration]) -> Self {
        if samples.is_empty() {
            return Self {
                count: 0,
                first_us: 0.0,
                min_us: 0.0,
                median_us: 0.0,
                p90_us: 0.0,
                max_us: 0.0,
                mean_us: 0.0,
            };
        }
        let us = |d: &Duration| d.as_secs_f64() * 1e6;
        let mut sorted: Vec<f64> = samples.iter().map(us).collect();
        sorted.sort_by(f64::total_cmp);
        // Nearest rank: the smallest sample at or above the quantile.
        let rank = |q: f64| {
            let i = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
            sorted[i - 1]
        };
        Self {
            count: u32::try_from(samples.len()).unwrap_or(u32::MAX),
            first_us: us(&samples[0]),
            min_us: sorted[0],
            median_us: rank(0.5),
            p90_us: rank(0.9),
            max_us: sorted[sorted.len() - 1],
            mean_us: sorted.iter().sum::<f64>() / sorted.len() as f64,
        }
    }
}
