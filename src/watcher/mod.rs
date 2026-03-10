use std::time::{Duration, Instant, SystemTime};

/// Watcher operational state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatcherState {
    /// File watcher is running and receiving events.
    Active,
    /// File watcher encountered errors but partial operation continues.
    Degraded,
    /// File watcher is not running.
    Off,
}

/// Snapshot of file watcher status for health reporting.
#[derive(Clone, Debug)]
pub struct WatcherInfo {
    pub state: WatcherState,
    pub events_processed: u64,
    pub last_event_at: Option<SystemTime>,
    pub debounce_window_ms: u64,
}

impl Default for WatcherInfo {
    fn default() -> Self {
        WatcherInfo {
            state: WatcherState::Off,
            events_processed: 0,
            last_event_at: None,
            debounce_window_ms: 200,
        }
    }
}

/// Tracks event bursts to adaptively extend the debounce window.
///
/// Debounce logic:
/// - Base window: 200ms
/// - Burst window: 500ms (when >BURST_THRESHOLD events in a 200ms window)
/// - Resets to 200ms after QUIET_SECS of inactivity
pub struct BurstTracker {
    pub event_count: u32,
    pub window_start: Instant,
    pub last_event_at: Instant,
    pub extended: bool,
}

impl BurstTracker {
    const BURST_THRESHOLD: u32 = 3;
    const BASE_MS: u64 = 200;
    const BURST_MS: u64 = 500;
    const QUIET_SECS: u64 = 5;

    /// Create a new BurstTracker with all counters at zero.
    pub fn new() -> Self {
        let now = Instant::now();
        BurstTracker {
            event_count: 0,
            window_start: now,
            last_event_at: now,
            extended: false,
        }
    }

    /// Record an event at the given instant, updating burst state.
    ///
    /// Window logic: if `now - window_start > BASE_MS`, start a new window
    /// and reset count to 1. Otherwise increment count.
    /// If count exceeds BURST_THRESHOLD, set extended=true.
    /// Always updates last_event_at.
    pub fn update(&mut self, now: Instant) {
        let window_duration = now.duration_since(self.window_start);
        if window_duration > Duration::from_millis(Self::BASE_MS) {
            // Start a new window
            self.window_start = now;
            self.event_count = 1;
            self.extended = false;
        } else {
            self.event_count += 1;
            if self.event_count > Self::BURST_THRESHOLD {
                self.extended = true;
            }
        }
        self.last_event_at = now;
    }

    /// Returns the effective debounce window in milliseconds.
    ///
    /// - If last event was more than QUIET_SECS ago, return BASE_MS (quiet reset)
    /// - If in burst mode (extended=true), return BURST_MS
    /// - Otherwise return BASE_MS
    pub fn effective_debounce_ms(&self) -> u64 {
        let since_last = self.last_event_at.elapsed();
        if since_last > Duration::from_secs(Self::QUIET_SECS) {
            return Self::BASE_MS;
        }
        if self.extended {
            Self::BURST_MS
        } else {
            Self::BASE_MS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_watcher_state_variants() {
        // All three variants exist and are distinct
        let active = WatcherState::Active;
        let degraded = WatcherState::Degraded;
        let off = WatcherState::Off;
        assert_ne!(active, degraded);
        assert_ne!(active, off);
        assert_ne!(degraded, off);
    }

    #[test]
    fn test_watcher_info_default() {
        let info = WatcherInfo::default();
        assert_eq!(info.state, WatcherState::Off);
        assert_eq!(info.events_processed, 0);
        assert!(info.last_event_at.is_none());
        assert_eq!(info.debounce_window_ms, 200);
    }

    #[test]
    fn test_burst_tracker_new() {
        let tracker = BurstTracker::new();
        assert_eq!(tracker.event_count, 0);
        assert!(!tracker.extended);
    }

    #[test]
    fn test_burst_tracker_extends_window() {
        // 4 events within 200ms -> extended=true, effective=500
        let mut tracker = BurstTracker::new();
        let start = Instant::now();
        // Simulate 4 rapid events within the same 200ms window
        tracker.update(start + Duration::from_millis(10));
        tracker.update(start + Duration::from_millis(20));
        tracker.update(start + Duration::from_millis(30));
        tracker.update(start + Duration::from_millis(40));
        assert!(tracker.extended, "4 events in window should trigger burst");
        assert_eq!(tracker.effective_debounce_ms(), 500);
    }

    #[test]
    fn test_burst_tracker_resets_after_quiet() {
        // After last event > 5s ago, effective should return 200
        let mut tracker = BurstTracker::new();
        let past = Instant::now() - Duration::from_secs(10);
        // Set last_event_at to 10s ago by creating tracker with manual past
        // We simulate this by forcing extended=true and setting last_event_at in the past
        tracker.extended = true;
        tracker.last_event_at = past;
        assert_eq!(
            tracker.effective_debounce_ms(),
            200,
            "after quiet period, should reset to 200ms"
        );
    }

    #[test]
    fn test_burst_tracker_new_window_resets_count() {
        // An event after >200ms gap should start a fresh window with count=1, extended=false
        let mut tracker = BurstTracker::new();
        let t0 = Instant::now();
        // First burst: 4 events
        tracker.update(t0 + Duration::from_millis(10));
        tracker.update(t0 + Duration::from_millis(20));
        tracker.update(t0 + Duration::from_millis(30));
        tracker.update(t0 + Duration::from_millis(40));
        assert!(tracker.extended, "should be extended after burst");

        // Event after 300ms gap
        tracker.update(t0 + Duration::from_millis(350));
        assert_eq!(tracker.event_count, 1, "count should reset to 1 after gap");
        assert!(!tracker.extended, "extended should reset after new window");
    }

    #[test]
    fn test_burst_tracker_base_debounce_no_burst() {
        // Under threshold: effective should remain 200ms
        let mut tracker = BurstTracker::new();
        let t0 = Instant::now();
        tracker.update(t0 + Duration::from_millis(10));
        tracker.update(t0 + Duration::from_millis(20));
        // Only 2 events, under BURST_THRESHOLD of 3
        assert!(!tracker.extended);
        assert_eq!(tracker.effective_debounce_ms(), 200);
    }
}
