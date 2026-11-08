//! Real-time Runtime Performance Telemetry, FPS Tracking, and ANR Watchdog.
//!
//! Provides rolling FPS estimation, 1% low frame-time metrics, memory consumption
//! profiling, and standard Android 5-second ANR (Application Not Responding) detection.

use std::collections::VecDeque;

/// Telemetry metrics for display and diagnostics.
#[derive(Debug, Clone, Default)]
pub struct RuntimeTelemetrySnapshot {
    pub average_fps: f32,
    pub one_percent_low_fps: f32,
    pub frame_time_ms: f32,
    pub heap_allocated_bytes: usize,
    pub total_frames_rendered: u64,
    pub anr_detected: bool,
}

/// ANR (Application Not Responding) watchdog tracking main-thread dispatch duration.
#[derive(Debug, Clone)]
pub struct AnrWatchdog {
    pub timeout_threshold_ms: u64,
    pub dispatch_started_at_ms: Option<u64>,
    pub last_anr_timestamp_ms: Option<u64>,
}

impl Default for AnrWatchdog {
    fn default() -> Self {
        Self {
            timeout_threshold_ms: 5000, // Android default 5 seconds
            dispatch_started_at_ms: None,
            last_anr_timestamp_ms: None,
        }
    }
}

impl AnrWatchdog {
    pub fn new(timeout_threshold_ms: u64) -> Self {
        Self {
            timeout_threshold_ms,
            dispatch_started_at_ms: None,
            last_anr_timestamp_ms: None,
        }
    }

    /// Marks the start of a main thread event or lifecycle callback.
    pub fn on_dispatch_start(&mut self, now_ms: u64) {
        self.dispatch_started_at_ms = Some(now_ms);
    }

    /// Marks the completion of the main thread dispatch.
    pub fn on_dispatch_finish(&mut self) {
        self.dispatch_started_at_ms = None;
    }

    /// Checks whether the current execution has exceeded the ANR threshold.
    pub fn check_anr(&mut self, now_ms: u64) -> bool {
        if let Some(start) = self.dispatch_started_at_ms {
            if now_ms.saturating_sub(start) >= self.timeout_threshold_ms {
                self.last_anr_timestamp_ms = Some(now_ms);
                return true;
            }
        }
        false
    }
}

/// Rolling FPS and frame-time telemetry tracker.
pub struct PerformanceTelemetry {
    recent_frame_times_ms: VecDeque<f32>,
    max_history_samples: usize,
    total_frames_rendered: u64,
    pub watchdog: AnrWatchdog,
}

impl Default for PerformanceTelemetry {
    fn default() -> Self {
        Self::new(60)
    }
}

impl PerformanceTelemetry {
    pub fn new(history_samples: usize) -> Self {
        Self {
            recent_frame_times_ms: VecDeque::with_capacity(history_samples),
            max_history_samples: history_samples.max(10),
            total_frames_rendered: 0,
            watchdog: AnrWatchdog::default(),
        }
    }

    /// Records a newly completed frame with its execution duration in milliseconds.
    pub fn record_frame(&mut self, duration_ms: f32) {
        if self.recent_frame_times_ms.len() >= self.max_history_samples {
            self.recent_frame_times_ms.pop_front();
        }
        self.recent_frame_times_ms.push_back(duration_ms.max(0.1));
        self.total_frames_rendered += 1;
    }

    /// Calculates the rolling average FPS.
    pub fn current_fps(&self) -> f32 {
        if self.recent_frame_times_ms.is_empty() {
            return 0.0;
        }
        let total_ms: f32 = self.recent_frame_times_ms.iter().sum();
        let avg_frame_ms = total_ms / (self.recent_frame_times_ms.len() as f32);
        if avg_frame_ms > 0.0 {
            1000.0 / avg_frame_ms
        } else {
            0.0
        }
    }

    /// Calculates 1% low FPS based on the 99th percentile slowest frame times.
    pub fn one_percent_low_fps(&self) -> f32 {
        if self.recent_frame_times_ms.len() < 10 {
            return self.current_fps();
        }

        let mut sorted: Vec<f32> = self.recent_frame_times_ms.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // 99th percentile index
        let p99_idx = ((sorted.len() as f32) * 0.99) as usize;
        let slowest_frame_ms = sorted.get(p99_idx).copied().unwrap_or(16.6);

        if slowest_frame_ms > 0.0 {
            1000.0 / slowest_frame_ms
        } else {
            0.0
        }
    }

    /// Produces a telemetry snapshot for diagnostics or Swift UI display.
    pub fn snapshot(&self, heap_allocated_bytes: usize, now_ms: u64) -> RuntimeTelemetrySnapshot {
        let mut watchdog_copy = self.watchdog.clone();
        RuntimeTelemetrySnapshot {
            average_fps: self.current_fps(),
            one_percent_low_fps: self.one_percent_low_fps(),
            frame_time_ms: self.recent_frame_times_ms.back().copied().unwrap_or(0.0),
            heap_allocated_bytes,
            total_frames_rendered: self.total_frames_rendered,
            anr_detected: watchdog_copy.check_anr(now_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_fps_calculation() {
        let mut telemetry = PerformanceTelemetry::new(60);
        // Simulate 60 frames taking 16.66ms (~60 FPS)
        for _ in 0..60 {
            telemetry.record_frame(16.66);
        }

        let fps = telemetry.current_fps();
        assert!(fps >= 59.0 && fps <= 61.0, "FPS should be approx 60, got {}", fps);

        let p1_low = telemetry.one_percent_low_fps();
        assert!(p1_low >= 59.0 && p1_low <= 61.0, "1% low should be approx 60, got {}", p1_low);
    }

    #[test]
    fn anr_watchdog_triggering() {
        let mut watchdog = AnrWatchdog::new(5000);
        assert!(!watchdog.check_anr(1000));

        // Start dispatch at 10,000ms
        watchdog.on_dispatch_start(10_000);

        // Check at 14,000ms (4s elapsed) -> Not triggered
        assert!(!watchdog.check_anr(14_000));

        // Check at 15,500ms (5.5s elapsed) -> Triggered ANR!
        assert!(watchdog.check_anr(15_500));
        assert_eq!(watchdog.last_anr_timestamp_ms, Some(15_500));

        // Finish dispatch
        watchdog.on_dispatch_finish();
        assert!(!watchdog.check_anr(20_000));
    }
}
