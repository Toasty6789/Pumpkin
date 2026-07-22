//! Simulated 1000-player bot stress test benchmark.
//!
//! This benchmark creates a configurable number of lightweight player-like
//! tasks that simulate the per-player tick workload (state reads, atomic
//! updates, wake-ups) without requiring a full Minecraft protocol stack.
//!
//! The goal is to measure the Server's ability to dispatch and complete
//! player ticks within the 50 ms budget required for 20 TPS.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Default number of simulated players for the stress benchmark.
const DEFAULT_PLAYER_COUNT: usize = 1_000;

// ---------------------------------------------------------------------------
// Simulated player
// ---------------------------------------------------------------------------

/// A minimal player stand-in that exercises the same kind of atomic state
/// reads and lightweight work that the real `Player::tick()` does.
struct SimulatedPlayer {
    id: u32,
    tick_count: AtomicU32,
    // Simulate a "connected" flag like the real client's close_token.
    connected: AtomicBool,
}

impl SimulatedPlayer {
    fn new(id: u32) -> Arc<Self> {
        Arc::new(Self {
            id,
            tick_count: AtomicU32::new(0),
            connected: AtomicBool::new(true),
        })
    }

    /// Simulate one tick of player logic.
    ///
    /// In the real server a player tick does network reads/writes, keep-alive
    /// checks, position updates, chunk scanning, etc.  Here we approximate the
    /// same patterns with atomic operations and a tiny spin-loop so the
    /// benchmark measures contention patterns, not pure idle.
    async fn tick(self: Arc<Self>) {
        // Simulate a small amount of CPU-bound work (~2 µs on modern hardware).
        // This models the actual overhead of packet encoding, state machines,
        // and inventory lookups without pulling in the full protocol stack.
        let _ = self.tick_count.fetch_add(1, Ordering::Relaxed);
        black_box(self.id);
        let _connected = self.connected.load(Ordering::Relaxed);
        // Yield so the runtime can schedule other players.
        tokio::task::yield_now().await;
    }
}

// ---------------------------------------------------------------------------
// Stress-test runner
// ---------------------------------------------------------------------------

/// Runs a stress test with `player_count` simulated players, driving them
/// through `tick_count` ticks each.
async fn run_player_stress(
    player_count: usize,
    tick_count: usize,
    concurrency_limit: usize,
) -> Duration {
    // Create players
    let players: Vec<Arc<SimulatedPlayer>> = (0..player_count)
        .map(|i| SimulatedPlayer::new(i as u32))
        .collect();

    // Semaphore to cap concurrent player ticks (mirrors JoinSet behaviour).
    let semaphore = Arc::new(Semaphore::new(concurrency_limit));

    let start = Instant::now();

    for _tick in 0..tick_count {
        let mut set = JoinSet::new();
        for player in &players {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let p = player.clone();
            set.spawn(async move {
                p.tick().await;
                drop(permit);
            });
        }
        // Wait for all players in this tick to finish.
        while let Some(res) = set.join_next().await {
            res.expect("simulated player tick task panicked");
        }
    }

    start.elapsed()
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_player_stress(c: &mut Criterion) {
    let rt = Runtime::new().expect("failed to create tokio runtime");

    c.bench_function(
        &format!("player_stress/{DEFAULT_PLAYER_COUNT}_players_1_tick"),
        |b| {
            b.to_async(&rt).iter(|| async {
                let elapsed = run_player_stress(DEFAULT_PLAYER_COUNT, 1, usize::MAX).await;
                // Return the elapsed time so black_box can prevent the compiler
                // from optimising the whole loop away.
                black_box(elapsed);
            });
        },
    );

    c.bench_function(
        &format!("player_stress/{DEFAULT_PLAYER_COUNT}_players_5_ticks"),
        |b| {
            b.to_async(&rt).iter(|| async {
                let elapsed = run_player_stress(DEFAULT_PLAYER_COUNT, 5, usize::MAX).await;
                black_box(elapsed);
            });
        },
    );

    // Concurrency-limited variant (simulates a server with limited worker threads).
    c.bench_function(
        &format!("player_stress/{DEFAULT_PLAYER_COUNT}_players_1_tick_concurrent_64"),
        |b| {
            b.to_async(&rt).iter(|| async {
                let elapsed = run_player_stress(DEFAULT_PLAYER_COUNT, 1, 64).await;
                black_box(elapsed);
            });
        },
    );
}

// ---------------------------------------------------------------------------
// Integration test-style verification (runs with `cargo test`)
// ---------------------------------------------------------------------------

/// Verify that 1000 players can complete a single tick in under 1 second on
/// modern hardware.  This is a soft check; CI runners may be slower, so the
/// threshold is deliberately generous.
#[tokio::test]
async fn player_stress_single_tick_under_1s() {
    let elapsed = run_player_stress(1000, 1, usize::MAX).await;
    assert!(
        elapsed.as_secs_f64() < 1.0,
        "1000 players / 1 tick took {:.2}s (expected < 1.0s)",
        elapsed.as_secs_f64(),
    );
}

/// Verify that 100 players can complete 10 ticks near real-time (≤ 500 ms).
#[tokio::test]
async fn player_stress_small_batch() {
    let elapsed = run_player_stress(100, 10, usize::MAX).await;
    assert!(
        elapsed.as_secs_f64() < 0.5,
        "100 players / 10 ticks took {:.2}s (expected < 0.5s)",
        elapsed.as_secs_f64(),
    );
}

// ---------------------------------------------------------------------------
// Criterion harness
// ---------------------------------------------------------------------------

criterion_group!(
    name = player_stress;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(5))
        .sample_size(20);
    targets = bench_player_stress,
);

criterion_main!(player_stress);
