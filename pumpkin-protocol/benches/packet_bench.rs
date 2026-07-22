// =============================================================================
// Criterion Benchmarks — Pumpkin Protocol Packet Encode/Decode
// =============================================================================
//
// Measures throughput for encoding and decoding common Minecraft protocol
// packet types. Results help identify performance regressions in the
// serialization layer.
//
// Run with:
//   cargo bench --package pumpkin-protocol
//
// To run a specific benchmark group:
//   cargo bench --package pumpkin-protocol "encode_serverbound"
//
// =============================================================================

use std::hint::black_box;
use std::io::Cursor;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use pumpkin_protocol::{
    ClientPacket, ServerPacket,
    codec::var_int::VarInt,
    java::{
        client::{play::CKeepAlive, status::CStatusResponse},
        server::{
            handshake::SHandShake,
            play::{SConfirmTeleport, SKeepAlive},
        },
    },
    ser::NetworkWriteExt,
};
use pumpkin_util::version::JavaMinecraftVersion;

const BENCH_VERSION: JavaMinecraftVersion = JavaMinecraftVersion::V_1_21_4;

// ---------------------------------------------------------------------------
// Helper: pre-encode a ServerPacket into raw bytes for decode benchmarks
// ---------------------------------------------------------------------------
fn encode_raw(writer: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut buf = Vec::new();
    writer(&mut buf);
    buf
}

// =========================================================================
// Benchmark: encode clientbound packets
// =========================================================================

fn bench_encode_clientbound(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_clientbound");
    group.throughput(Throughput::Elements(1));

    let keep_alive = CKeepAlive::new(999);

    let status_response = CStatusResponse::new(String::from(
        r#"{"description":"A Pumpkin Server","players":{"max":100,"online":0},"version":{"name":"1.21.4","protocol":769}}"#,
    ));

    group.bench_function(BenchmarkId::new("encode", "keep_alive"), |b| {
        b.iter(|| {
            let mut buf = Vec::with_capacity(16);
            black_box(keep_alive.write_packet_data(&mut buf, &BENCH_VERSION))
        });
    });
    group.bench_function(BenchmarkId::new("encode", "status_response"), |b| {
        b.iter(|| {
            let mut buf = Vec::with_capacity(256);
            black_box(status_response.write_packet_data(&mut buf, &BENCH_VERSION))
        });
    });

    group.finish();
}

// =========================================================================
// Benchmark: encode serverbound packets that implement ClientPacket
// =========================================================================

fn bench_encode_serverbound(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_serverbound");
    group.throughput(Throughput::Elements(1));

    let handshake = SHandShake {
        protocol_version: VarInt(767),
        server_address: Box::from("localhost"),
        server_port: 25565,
        next_state: pumpkin_protocol::ConnectionState::Login,
    };

    group.bench_with_input(
        BenchmarkId::new("encode", "handshake"),
        &handshake,
        |b, pkt| {
            b.iter(|| {
                let mut buf = Vec::with_capacity(64);
                black_box(pkt.write_packet_data(&mut buf, &BENCH_VERSION))
            });
        },
    );

    group.finish();
}

// =========================================================================
// Benchmark: decode serverbound packets (via raw byte construction)
// =========================================================================

fn bench_decode_serverbound(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode_serverbound");
    group.throughput(Throughput::Elements(1));

    // Pre-encode each packet type as raw bytes
    let keep_alive_bytes = encode_raw(|buf| {
        buf.write_i64_be(123_456).unwrap();
    });

    let confirm_teleport_bytes = encode_raw(|buf| {
        buf.write_var_int(&VarInt(42)).unwrap();
    });

    group.bench_with_input(
        BenchmarkId::new("decode", "keep_alive"),
        &keep_alive_bytes.as_slice(),
        |b, data| {
            b.iter(|| {
                let mut cursor = Cursor::new(black_box(data));
                black_box(SKeepAlive::read(&mut cursor, &BENCH_VERSION))
            });
        },
    );
    group.bench_with_input(
        BenchmarkId::new("decode", "confirm_teleport"),
        &confirm_teleport_bytes.as_slice(),
        |b, data| {
            b.iter(|| {
                let mut cursor = Cursor::new(black_box(data));
                black_box(SConfirmTeleport::read(&mut cursor, &BENCH_VERSION))
            });
        },
    );

    group.finish();
}

// =========================================================================
// Benchmark: encode throughput (many packets sequentially)
// =========================================================================

fn bench_encode_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_throughput");
    group.throughput(Throughput::Elements(1_000));

    let packet = CKeepAlive::new(42);

    group.bench_function("keep_alive_1000x", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let mut buf = Vec::with_capacity(16);
                black_box(packet.write_packet_data(&mut buf, &BENCH_VERSION));
            }
        });
    });

    group.finish();
}

// =========================================================================
// Benchmark: decode throughput
// =========================================================================

fn bench_decode_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode_throughput");
    group.throughput(Throughput::Elements(1_000));

    let raw = encode_raw(|buf| {
        buf.write_i64_be(42).unwrap();
    });

    group.bench_function("keep_alive_1000x", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let mut cursor = Cursor::new(black_box(&raw[..]));
                let _ = SKeepAlive::read(&mut cursor, &BENCH_VERSION);
            }
        });
    });

    group.finish();
}

// =========================================================================
// Criterion harness
// =========================================================================

criterion_group!(
    name = packet_benches;
    config = Criterion::default()
        .warm_up_time(std::time::Duration::from_secs(2))
        .measurement_time(std::time::Duration::from_secs(5))
        .sample_size(100);
    targets =
        bench_encode_clientbound,
        bench_encode_serverbound,
        bench_decode_serverbound,
        bench_encode_throughput,
        bench_decode_throughput,
);

criterion_main!(packet_benches);
