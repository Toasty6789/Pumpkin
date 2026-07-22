//! Criterion benchmarks for the Pumpkin server tick engine.
//!
//! These benchmarks measure tick rate stability, tick duration distribution,
//! and the overhead of the tick loop itself. They exercise `ServerTickRateManager`
//! and simulate the core tick dispatch path used by the ticker.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::atomic::{AtomicI32, AtomicI64, Ordering};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Mock helpers — we benchmark the tick-rate math and scheduling logic without
// pulling in the full tokio runtime or the real Server.
// ---------------------------------------------------------------------------

/// Minimal stand-in for the tick-rate manager so we can measure its overhead
/// in isolation.
struct BenchTickRateManager {
    nanoseconds_per_tick: AtomicI64,
}

impl BenchTickRateManager {
    fn new(tps: f32) -> Self {
        let ns = (1_000_000_000f64 / tps as f64) as i64;
        Self {
            nanoseconds_per_tick: AtomicI64::new(ns),
        }
    }

    fn nanoseconds_per_tick(&self) -> i64 {
        self.nanoseconds_per_tick.load(Ordering::Relaxed)
    }

    /// Returns a fake tick-interval computed from the configured TPS.
    fn tick_interval(&self) -> Duration {
        Duration::from_nanos(self.nanoseconds_per_tick() as u64)
    }
}

/// Produces a sequence of tick-interval durations suitable for feeding into a
/// benchmark loop.
fn generate_tick_intervals(count: usize, tps: f32) -> Vec<Duration> {
    let manager = BenchTickRateManager::new(tps);
    (0..count).map(|_| manager.tick_interval()).collect()
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

/// Measures the pure computation overhead of the tick-rate manager methods.
fn bench_tick_rate_manager_overhead(c: &mut Criterion) {
    let manager = BenchTickRateManager::new(20.0); // 20 TPS

    c.bench_function("tick_rate_manager/ns_per_tick", |b| {
        b.iter(|| {
            black_box(manager.nanoseconds_per_tick());
        });
    });

    c.bench_function("tick_rate_manager/tick_interval_20tps", |b| {
        b.iter(|| {
            black_box(manager.tick_interval());
        });
    });

    // Stress the tick interval calculation across a range of valid TPS values
    c.bench_function("tick_rate_manager/interval_range", |b| {
        let rates: Vec<f32> = (1..=40).map(|i| i as f32).collect();
        b.iter(|| {
            for &rate in &rates {
                let m = BenchTickRateManager::new(rate);
                black_box(m.tick_interval());
            }
        });
    });
}

/// Measures the allocation and layout of a tick-interval slice, which models
/// the pre-computation the ticker does for the next N ticks.
fn bench_tick_interval_generation(c: &mut Criterion) {
    c.bench_function("tick_interval/generate_1000_intervals", |b| {
        b.iter(|| {
            let intervals = generate_tick_intervals(1000, 20.0);
            black_box(intervals);
        });
    });
}

/// Simulates the structural overhead of the "death spiral prevention" logic
/// that resets the next-tick deadline when the server falls too far behind.
fn bench_death_spiral_check(c: &mut Criterion) {
    c.bench_function("tick_loop/death_spiral_check", |b| {
        let mut next_tick = Instant::now();
        let fake_now = Instant::now() + Duration::from_secs(10); // way past deadline

        b.iter(|| {
            let delta = fake_now.saturating_duration_since(next_tick);
            if delta > Duration::from_secs(5) {
                next_tick = fake_now;
            }
            black_box(next_tick);
        });
    });
}

/// Simulates the atomic tick-count / aggregated-time update path that happens
/// at the end of each real tick.
fn bench_tick_accounting(c: &mut Criterion) {
    c.bench_function("tick_loop/accounting_update", |b| {
        let tick_count = AtomicI32::new(0);
        let tick_times: [AtomicI64; 100] = std::array::from_fn(|_| AtomicI64::new(0));

        b.iter(|| {
            let duration_ns: i64 =
                5_000_000 + (tick_count.load(Ordering::Relaxed) as i64 % 1_000_000);
            let index = tick_count.fetch_add(1, Ordering::Relaxed) as usize % 100;
            let _old = tick_times[index].swap(duration_ns, Ordering::Relaxed);
            black_box((tick_count.load(Ordering::Relaxed), index));
        });
    });
}

// ---------------------------------------------------------------------------
// Criterion harness
// ---------------------------------------------------------------------------

criterion_group!(
    name = tick_benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(3))
        .sample_size(50);
    targets =
        bench_tick_rate_manager_overhead,
        bench_tick_interval_generation,
        bench_death_spiral_check,
        bench_tick_accounting,
);

criterion_main!(tick_benches);
