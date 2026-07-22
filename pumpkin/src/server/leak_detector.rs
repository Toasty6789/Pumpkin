//! Arc reference-count diagnostic tracking for leak detection.
//!
//! This module provides a diagnostic facility that periodically samples the
//! strong-reference counts of key `Arc`-wrapped structures — players, chunks,
//! worlds — and reports any that appear to be leaking (i.e., references that
//! persist after the owning subsystem has cleaned up).
//!
//! # Design
//!
//! On every tick (or at a configurable interval) the `LeakDetector` walks the
//! current set of tracked resources and records their strong-count snapshots.
//! After a grace period, survivors are reported via `tracing::warn!` so
//! operators can investigate.
//!
//! # Usage
//!
//! ```ignore
//! use crate::server::leak_detector::LeakDetector;
//!
//! let detector = LeakDetector::new();
//!
//! // Register a resource to watch.
//! let player: Arc<Player> = /* ... */;
//! detector.watch("player::Toast", Arc::downgrade(&player));
//!
//! // Sample during the tick — logs any suspicious survivors.
//! detector.tick();
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::warn;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A watched resource together with its metadata.
struct TrackedResource {
    /// Human-readable label (e.g. `"chunk::(42,-7)"`, `"player::uuid-…"`).
    label: &'static str,
    /// Weak reference to the tracked `Arc`.  When the `Arc` is dropped this
    /// upgrade will return `None` and the entry is automatically removed.
    weak: std::sync::Weak<()>,
    /// Strong count observed at the start of the current grace period.
    snapshot_count: usize,
    /// Wall-clock time when the resource was first observed in a "possibly
    /// leaked" state.
    first_seen: Option<Instant>,
}

/// Diagnostic tracker that samples `Arc` strong-reference counts at a
/// configurable interval and reports suspected leaks.
pub struct LeakDetector {
    /// Label-indexed tracked resources.
    resources: std::sync::Mutex<HashMap<&'static str, TrackedResource>>,
    /// How many ticks a survivor must persist before it is reported.
    grace_ticks: u32,
    /// Interval (in ticks) between full sweep-and-report passes.
    sample_interval: u32,
    /// Counter of ticks processed.
    tick_counter: AtomicU64,
}

impl Default for LeakDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl LeakDetector {
    /// Create a new leak detector with default parameters.
    ///
    /// - grace period: 600 ticks (≈ 30 s at 20 TPS)
    /// - sample interval: 20 ticks (every 1 s)
    #[must_use]
    pub fn new() -> Self {
        Self {
            resources: std::sync::Mutex::new(HashMap::new()),
            grace_ticks: 600,
            sample_interval: 20,
            tick_counter: AtomicU64::new(0),
        }
    }

    /// Create a leak detector with non-default parameters.
    #[must_use]
    pub fn with_params(grace_ticks: u32, sample_interval: u32) -> Self {
        Self {
            resources: std::sync::Mutex::new(HashMap::new()),
            grace_ticks,
            sample_interval,
            tick_counter: AtomicU64::new(0),
        }
    }

    /// Register a resource to be tracked.
    ///
    /// `label` should be unique and descriptive; it is used as the map key so
    /// re-registering a label silently overwrites the previous entry.
    pub fn watch(&self, label: &'static str, weak: std::sync::Weak<()>) {
        if let Ok(mut guard) = self.resources.lock() {
            let snapshot = weak.strong_count();
            guard.insert(
                label,
                TrackedResource {
                    label,
                    weak,
                    snapshot_count: snapshot,
                    first_seen: None,
                },
            );
        }
    }

    /// Remove a previously registered label so it is no longer tracked.
    pub fn unwatch(&self, label: &'static str) {
        if let Ok(mut guard) = self.resources.lock() {
            guard.remove(label);
        }
    }

    /// Periodic tick — samples strong counts and reports survivors.
    ///
    /// Should be called once per server tick (or every N ticks, controlled by
    /// `sample_interval`).
    pub fn tick(&self) {
        let tick = self.tick_counter.fetch_add(1, Ordering::Relaxed);
        if !(tick as u32).is_multiple_of(self.sample_interval) {
            return;
        }

        let Ok(mut guard) = self.resources.lock() else {
            return;
        };

        let now = Instant::now();

        guard.retain(|_label, tracked| {
            // If the Arc was dropped, remove the entry.
            if tracked.weak.strong_count() == 0 {
                return false; // drop it
            }

            let current_count = tracked.weak.strong_count();

            // If the count hasn't changed since last sample, it might be stable
            // (expected) or leaked (if it should have been dropped).
            if current_count == tracked.snapshot_count && current_count > 1 {
                match tracked.first_seen {
                    None => {
                        // First observation — start the grace timer.
                        tracked.first_seen = Some(now);
                    }
                    Some(first) if first.elapsed() > self.grace_duration() => {
                        // The resource has survived past the grace period.
                        warn!(
                            "LeakDetector: possible leak for `{}` — strong_count={} \
                             (above the expected baseline of 1), has been alive for ≥{} ticks",
                            tracked.label, current_count, self.grace_ticks,
                        );
                        // Reset the timer to avoid spamming every tick.
                        tracked.first_seen = Some(now);
                    }
                    _ => {
                        // Still within the grace period.
                    }
                }
            } else {
                // Count changed — it's actively being used; reset the timer.
                tracked.first_seen = None;
            }

            tracked.snapshot_count = current_count;
            true
        });
    }

    /// Return the number of currently tracked resources.
    #[must_use]
    pub fn tracked_count(&self) -> usize {
        self.resources.lock().map_or(0, |g| g.len())
    }

    /// Generate a diagnostic report of all currently tracked resources and
    /// their strong counts.
    #[must_use]
    pub fn report(&self) -> Vec<(&'static str, usize)> {
        self.resources
            .lock()
            .map(|g| {
                g.values()
                    .map(|t| (t.label, t.weak.strong_count()))
                    .collect()
            })
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn grace_duration(&self) -> Duration {
        // At 20 TPS each tick is 50 ms.
        Duration::from_millis(u64::from(self.grace_ticks) * 50)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn track_and_release() {
        let detector = LeakDetector::with_params(10, 1);
        let data = Arc::new(());
        let weak = Arc::downgrade(&data);

        detector.watch("test::data", weak);
        assert_eq!(detector.tracked_count(), 1);

        // After the Arc is dropped, the next tick should clean it up.
        drop(data);
        detector.tick();
        assert_eq!(detector.tracked_count(), 0);
    }

    #[test]
    fn survivor_under_threshold_not_reported() {
        // Use a very long grace period so the survivor is NOT reported.
        let detector = LeakDetector::with_params(1_000_000, 1);
        let data = Arc::new(());
        let weak = Arc::downgrade(&data);

        detector.watch("test::survivor", weak);
        // Hold the Arc so it stays alive.
        let alive = [data.clone(), data];
        assert_eq!(Arc::strong_count(&alive[0]), 2);

        for _ in 0..5 {
            detector.tick();
        }

        // The entry should still be present (not cleaned because strong_count > 0).
        assert_eq!(detector.tracked_count(), 1);
    }

    #[test]
    fn explicit_unwatch() {
        let detector = LeakDetector::new();
        let data = Arc::new(());
        detector.watch("test::unwatch", Arc::downgrade(&data));
        assert_eq!(detector.tracked_count(), 1);

        detector.unwatch("test::unwatch");
        assert_eq!(detector.tracked_count(), 0);
    }
}
