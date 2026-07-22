//! The main server tick loop — the heartbeat of the Pumpkin server.
//!
//! This module is responsible for driving the server's update loop at a
//! configurable tick rate (default 20 TPS).  It provides:
//!
//! * **`Ticker`** — The async tick loop that dispatches `Server::tick()`.
//! * **`TickMetrics`** — Records min/max/avg tick times over configurable
//!   measurement windows so operators can observe performance.
//! * **Watchdog timer** — Detects when a single tick exceeds the 50 ms budget
//!   and emits a warning.
//! * **Backpressure handling** — When ticks exceed their budget the ticker
//!   dynamically adjusts the sleep-until deadline to protect the server from
//!   cascading delay (the "death spiral").

use crate::{
    STOP_INTERRUPT,
    plugin::server::{
        server_tick_end::ServerTickEndEvent, server_tick_start::ServerTickStartEvent,
    },
    server::Server,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::time::{Instant, sleep_until};
use tracing::{debug, warn};

// ===========================================================================
// TickMetrics
// ===========================================================================

/// Rolling-window tick-time statistics.
///
/// Records the minimum, maximum, and average tick execution times over the
/// most recent `window_size` ticks.  Writers are lock-free atomics; readers
/// see a consistent snapshot via [`Self::snapshot()`].
pub struct TickMetrics {
    /// Pre-allocated ring buffer of per-tick durations (nanoseconds).
    history: Box<[AtomicU32; 4096]>,
    /// The number of entries actually populated in `history` (caps the window).
    window_size: u16,
    /// Monotonically increasing tick counter that wraps around `history.len()`.
    cursor: AtomicU32,
}

impl TickMetrics {
    /// Create a new metrics tracker with the given window size (1–4096).
    ///
    /// # Panics
    ///
    /// Panics if `window_size` is 0 or exceeds 4096.
    #[must_use]
    pub fn new(window_size: u16) -> Self {
        assert!(
            window_size > 0 && window_size <= 4096,
            "TickMetrics window size must be 1..=4096, got {window_size}",
        );
        Self {
            history: Box::new(std::array::from_fn(|_| AtomicU32::new(0))),
            window_size,
            cursor: AtomicU32::new(0),
        }
    }

    /// Record the duration of a single tick.
    ///
    /// `duration_ns` is clamped to `u32::MAX` (~4.3 s) which is more than
    /// enough for any real tick.
    pub fn record(&self, duration_ns: u64) {
        let idx =
            self.cursor.fetch_add(1, Ordering::Release) as usize % usize::from(self.window_size);
        self.history[idx].store(
            duration_ns.min(u64::from(u32::MAX)) as u32,
            Ordering::Relaxed,
        );
    }

    /// Return a snapshot of the current window: `(min, max, avg)` in
    /// nanoseconds.  Returns all zeros if no ticks have been recorded yet.
    pub fn snapshot(&self) -> (u64, u64, f64) {
        let count = self
            .cursor
            .load(Ordering::Acquire)
            .min(u32::from(self.window_size));
        if count == 0 {
            return (0, 0, 0.0);
        }

        let mut min_ns = u64::MAX;
        let mut max_ns = 0u64;
        let mut sum: u64 = 0;

        for i in 0..(count as usize) {
            let v = self.history[i].load(Ordering::Relaxed) as u64;
            min_ns = min_ns.min(v);
            max_ns = max_ns.max(v);
            sum += v;
        }

        if min_ns == u64::MAX {
            min_ns = 0;
        }

        let avg_ns = sum as f64 / count as f64;
        (min_ns, max_ns, avg_ns)
    }

    /// Return the number of recorded ticks so far.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        u64::from(self.cursor.load(Ordering::Acquire))
    }
}

// ===========================================================================
// Watchdog
// ===========================================================================

/// A simple threshold-based watchdog that warns when a single tick exceeds
/// the expected budget.
struct TickWatchdog {
    /// Nanosecond threshold above which a warning is emitted.
    threshold_ns: u64,
    /// Suppress repeat warnings for the same tick (noise reduction).
    last_warned_at: std::sync::atomic::AtomicU64,
}

impl TickWatchdog {
    const fn new(threshold_ns: u64) -> Self {
        Self {
            threshold_ns,
            last_warned_at: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Check a tick duration and warn if it exceeds the threshold.
    /// Only warns once per tick to avoid spamming logs.
    fn check(&self, tick: u64, duration_ns: u64) {
        if duration_ns > self.threshold_ns {
            let last = self.last_warned_at.load(Ordering::Relaxed);
            if tick != last {
                self.last_warned_at.store(tick, Ordering::Relaxed);
                let budget_ms = self.threshold_ns as f64 / 1_000_000.0;
                let actual_ms = duration_ns as f64 / 1_000_000.0;
                warn!(
                    "Tick watchdog: tick #{tick} took {actual_ms:.2} ms \
                     (budget {budget_ms:.2} ms). Consider profiling.",
                );
            }
        }
    }
}

// ===========================================================================
// Ticker
// ===========================================================================

/// Drives the server's main loop, dispatching ticks at a fixed rate.
///
/// The ticker handles:
/// - Tick-rate-compliant scheduling (with "sprinting" support)
/// - Plugin event hooks (tick start / tick end)
/// - Tick-time accounting on the `Server` struct
/// - A death-spiral prevention mechanism that resets the deadline when the
///   server falls more than 5 s behind
/// - Watchdog warnings for ticks that exceed the 50 ms budget
/// - Optional `TickMetrics` recording for observability
pub struct Ticker;

impl Ticker {
    /// Run the tick loop.
    ///
    /// IMPORTANT: This must be spawned as a dedicated tokio task.
    ///
    /// The loop honours `STOP_INTERRUPT` so the server can shut down gracefully.
    pub async fn run(server: &Arc<Server>) {
        let mut next_tick = Instant::now();

        // Internal performance metrics — recorded alongside the server's own
        // aggregated tick times for finer-grained observability.
        let tick_metrics = TickMetrics::new(100);
        let watchdog = TickWatchdog::new(/* 50 ms budget */ 50_000_000);

        'ticker: loop {
            let tick_start_time = std::time::Instant::now();
            let manager = &server.tick_rate_manager;

            // -----------------------------------------------------------------
            // Phase 1 — Notify tick start
            // -----------------------------------------------------------------
            manager.tick();

            let tick_number = server.tick_count.load(Ordering::Relaxed);
            let _ = server
                .plugin_manager
                .fire(ServerTickStartEvent::new(tick_number))
                .await;

            // -----------------------------------------------------------------
            // Phase 2 — Execute the tick
            // -----------------------------------------------------------------
            if manager.is_sprinting() {
                manager.start_sprint_tick_work();
                server.tick().await;

                if manager.end_sprint_tick_work() {
                    manager.finish_tick_sprint(server);
                }
            } else {
                server.tick().await;
            }

            // -----------------------------------------------------------------
            // Phase 3 — Record tick duration
            // -----------------------------------------------------------------
            let tick_duration_nanos = tick_start_time.elapsed().as_nanos() as i64;

            let tick_number = server.tick_count.load(Ordering::Relaxed);
            let _ = server
                .plugin_manager
                .fire(ServerTickEndEvent::new(tick_number, tick_duration_nanos))
                .await;

            // Write to server's own rolling buffer.
            server.update_tick_times(tick_duration_nanos).await;

            // Write to the local high-resolution metrics.
            let tick_ns_u64 = tick_duration_nanos.max(0) as u64;
            tick_metrics.record(tick_ns_u64);

            // -----------------------------------------------------------------
            // Phase 4 — Watchdog check (20 TPS guarantee)
            // -----------------------------------------------------------------
            let tick_number_u64 = tick_number as u64;
            watchdog.check(tick_number_u64, tick_ns_u64);

            // -----------------------------------------------------------------
            // Phase 5 — Schedule next wake-up
            // -----------------------------------------------------------------
            let tick_interval = if manager.is_sprinting() {
                Duration::ZERO
            } else {
                Duration::from_nanos(manager.nanoseconds_per_tick() as u64)
            };

            // -----------------------------------------------------------------
            // Phase 6 — Backpressure handling
            //
            // If a tick exceeded its time budget we *still* advance `next_tick`
            // by the full interval rather than skipping ahead — this keeps the
            // long-term average tick rate correct (the "drifting deadline"
            // approach).  Vanilla Minecraft does the same.
            //
            // However, if the measured tick crossed *multiple* budget periods
            // we skip missed deadlines so a single slow tick doesn't cascade
            // into a stream of catch-up ticks.
            // -----------------------------------------------------------------
            if tick_ns_u64 > tick_interval.as_nanos() as u64 {
                // This tick overran its budget. Count how many periods were
                // missed and bump `next_tick` past them.
                let overrun = tick_ns_u64.saturating_sub(tick_interval.as_nanos() as u64);
                let missed_periods = overrun / tick_interval.as_nanos() as u64;
                if missed_periods > 1 {
                    // Skip ahead so we don't try to "catch up" all missed slots.
                    next_tick += tick_interval * (missed_periods as u32 + 1);
                } else {
                    next_tick += tick_interval;
                }
            } else {
                next_tick += tick_interval;
            }

            // -----------------------------------------------------------------
            // Phase 7 — Sleep until deadline (with cancellation)
            // -----------------------------------------------------------------
            tokio::select! {
                () = sleep_until(next_tick) => {},
                () = STOP_INTERRUPT.cancelled() => {
                    break 'ticker;
                }
            }

            // -----------------------------------------------------------------
            // Phase 8 — Death Spiral Prevention
            //
            // If the server is so overloaded that `next_tick` is more than 5 s
            // in the past, reset it to `now` to prevent a self-reinforcing
            // spiral of missed deadlines.
            // -----------------------------------------------------------------
            let now = Instant::now();
            if now.saturating_duration_since(next_tick) > Duration::from_secs(5) {
                debug!(
                    "Ticker death spiral prevention: resetting deadline (was {} ms behind)",
                    now.saturating_duration_since(next_tick).as_millis(),
                );
                next_tick = now;
            }
        }

        debug!("Ticker stopped");
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_metrics_empty() {
        let m = TickMetrics::new(10);
        let (min, max, avg) = m.snapshot();
        assert_eq!(min, 0);
        assert_eq!(max, 0);
        assert_eq!(avg, 0.0);
    }

    #[test]
    fn tick_metrics_record_and_snapshot() {
        let m = TickMetrics::new(5);
        m.record(10_000_000); // 10 ms
        m.record(20_000_000); // 20 ms
        m.record(15_000_000); // 15 ms

        let (min, max, avg) = m.snapshot();
        assert_eq!(min, 10_000_000);
        assert_eq!(max, 20_000_000);
        assert!((avg - 15_000_000.0).abs() < 0.001);
    }

    #[test]
    fn tick_metrics_window_size_clamping() {
        let m = TickMetrics::new(2);
        m.record(1);
        m.record(2);
        m.record(3); // this evicts `1`
        let (min, max, _avg) = m.snapshot();
        // Only the last 2 values are in the window.
        assert_eq!(min, 2);
        assert_eq!(max, 3);
    }

    #[test]
    fn tick_watchdog_threshold() {
        let w = TickWatchdog::new(50_000_000);
        // No panic — just checks that the logic doesn't explode.
        w.check(1, 10_000_000); // under budget → silent
        w.check(2, 60_000_000); // over budget → warns once
        w.check(2, 70_000_000); // same tick → suppressed
        w.check(3, 80_000_000); // new tick → warns
    }
}
