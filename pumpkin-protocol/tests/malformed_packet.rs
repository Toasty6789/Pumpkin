// =============================================================================
// Malformed Packet Resilience Tests — Pumpkin Protocol
// =============================================================================
//
// These tests verify that the packet decoder handles truncated, corrupted,
// and boundary-case data gracefully — returning errors instead of panicking
// or producing undefined behaviour.
//
// Test categories:
//   1. Truncated data (incomplete VarInt, partial payload)
//   2. Malformed VarInts (overlong, negative, too large)
//   3. Invalid compression headers
//   4. Out-of-bounds lengths (zero, negative, beyond max)
//   5. Random corruption (fuzz-like byte flips)
//
// =============================================================================

use std::io::Cursor;

use pumpkin_protocol::{
    ServerPacket,
    codec::var_int::VarInt,
    java::{
        packet_decoder::TCPNetworkDecoder,
        server::{
            handshake::SHandShake,
            login::SLoginStart,
            play::SKeepAlive,
        },
    },
    ser::{NetworkReadExt, NetworkWriteExt, ReadingError},
};
use pumpkin_util::version::JavaMinecraftVersion;

const TEST_VERSION: JavaMinecraftVersion = JavaMinecraftVersion::V_1_21_4;

// =========================================================================
// 1. Truncated data
// =========================================================================

/// Attempt to read a packet from an empty buffer.
#[test]
fn empty_buffer_fails() {
    let mut empty = Cursor::new(b"");
    let result = SKeepAlive::read(&mut empty, &TEST_VERSION);
    assert!(
        result.is_err(),
        "Reading from an empty buffer should fail"
    );
}

/// A partially-written VarInt should produce a read error, not a panic.
#[test]
fn truncated_varint_fails() {
    // VarInt encoding: 1 byte with MSB set => expects continuation
    let truncated = [0x80];
    let mut cursor = Cursor::new(&truncated);
    let result: Result<VarInt, _> = cursor.get_var_int();
    assert!(result.is_err(), "Truncated VarInt should fail");
}

/// A packet payload truncated mid-stream.
#[test]
fn truncated_keep_alive_payload_fails() {
    // A keep-alive payload is 8 bytes (i64). Provide only 4.
    let mut buf = Vec::new();
    buf.write_i64_be(42).unwrap();
    let truncated = &buf[..4];
    let mut cursor = Cursor::new(truncated);
    let result = SKeepAlive::read(&mut cursor, &TEST_VERSION);
    assert!(
        result.is_err(),
        "Truncated keep-alive payload should fail"
    );
}

/// Truncate a handshake packet (has a variable-length string).
#[test]
fn truncated_handshake_string_fails() {
    // Write a valid handshake, then truncate in the middle of the address
    let mut buf = Vec::new();
    buf.write_var_int(&VarInt(767)).unwrap();
    buf.write_string("very-long-server-address.example.com").unwrap();
    buf.write_u16_be(25565).unwrap();
    buf.write_var_int(&VarInt(2)).unwrap();
    let truncated = &buf[..buf.len().saturating_sub(10)];
    let mut cursor = Cursor::new(truncated);
    let result = SHandShake::read(&mut cursor, &TEST_VERSION);
    assert!(
        result.is_err(),
        "Truncated handshake string should fail"
    );
}

/// Attempt to read UUID bytes that aren't all present.
#[test]
fn truncated_login_start_uuid_fails() {
    let mut buf = Vec::new();
    buf.write_string("Player1").unwrap();
    // UUID needs 16 bytes — provide only 8
    buf.write_slice(&[0x00u8; 8]).unwrap();
    let mut cursor = Cursor::new(&buf);
    let result = SLoginStart::read(&mut cursor, &TEST_VERSION);
    assert!(result.is_err(), "Truncated UUID should fail");
}

// =========================================================================
// 2. Malformed VarInts
// =========================================================================

/// Overlong VarInt (more bytes than needed for the value).
#[test]
fn overlong_varint_does_not_panic() {
    // 5-byte VarInt 0x80 0x80 0x80 0x80 0x00 = 0 (overlong but valid)
    let overlong = [0x80, 0x80, 0x80, 0x80, 0x00];
    let mut cursor = Cursor::new(&overlong);
    let _result: Result<VarInt, _> = cursor.get_var_int();
    // Must not panic regardless of result
}

/// A VarInt that exceeds i32 range.
#[test]
fn oversized_varint_does_not_panic() {
    // 10 bytes all with MSB set
    let oversized = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01];
    let mut cursor = Cursor::new(&oversized);
    let _result: Result<VarInt, _> = cursor.get_var_int();
    // Must not panic
}

// =========================================================================
// 3. Invalid compression / framing
// =========================================================================

/// Garbage data in a compressed frame.
#[tokio::test]
async fn garbage_compressed_data_fails() {
    let mut frame = Vec::new();
    // Packet length = 20 (arbitrary, won't match actual content — fine)
    frame.write_var_int(&VarInt(20)).unwrap();
    // Data length (uncompressed size = 100)
    frame.write_var_int(&VarInt(100)).unwrap();
    // Garbage "compressed" data
    frame.extend_from_slice(&[0xFF, 0xFF, 0xFF]);

    let mut decoder = TCPNetworkDecoder::new(Cursor::new(&frame));
    decoder.set_compression(1); // threshold of 1 => everything gets decompressed
    let result = decoder.get_raw_packet().await;
    assert!(
        result.is_err(),
        "Garbage compressed data should produce an error"
    );
}

/// A frame claiming to be uncompressed (data length = 0) but with payload
/// larger than the threshold — protocol violation.
#[tokio::test]
async fn uncompressed_data_above_threshold_fails() {
    let payload = [0x00u8; 512];
    let mut frame = Vec::new();
    // Data length = 0 means "not compressed"
    frame.write_var_int(&VarInt(0)).unwrap();
    frame.extend_from_slice(&payload);

    let packet_len = frame.len() as i32;
    let mut full = Vec::new();
    full.write_var_int(&VarInt(packet_len)).unwrap();
    full.extend_from_slice(&frame);

    let mut decoder = TCPNetworkDecoder::new(Cursor::new(&full));
    decoder.set_compression(256);
    let result = decoder.get_raw_packet().await;
    assert!(
        result.is_err(),
        "Uncompressed data above threshold should fail"
    );
}

// =========================================================================
// 4. Out-of-bounds lengths
// =========================================================================

/// A decoder with an out-of-bounds packet length.
#[tokio::test]
async fn negative_packet_length_fails() {
    let mut frame = Vec::new();
    frame.write_var_int(&VarInt(-1)).unwrap();
    frame.push(0x00);

    let mut decoder = TCPNetworkDecoder::new(Cursor::new(&frame));
    let result = decoder.get_raw_packet().await;
    assert!(result.is_err(), "Negative packet length should fail");
}

/// A decoder presented with a length beyond MAX_PACKET_SIZE.
#[tokio::test]
async fn packet_length_too_large_fails() {
    let mut frame = Vec::new();
    frame.write_var_int(&VarInt(10_000_000)).unwrap();

    let mut decoder = TCPNetworkDecoder::new(Cursor::new(&frame));
    let result = decoder.get_raw_packet().await;
    assert!(result.is_err(), "Overly large packet length should fail");
}

// =========================================================================
// 5. Random corruption (fuzz-style)
// =========================================================================

/// Flip every bit in each byte of a serialised keep-alive payload and
/// verify the decoder never panics.
#[test]
fn bit_flips_do_not_cause_panic() {
    let mut buf = Vec::new();
    buf.write_i64_be(42).unwrap();

    for flip_pos in 0..buf.len() {
        let mut corrupted = buf.clone();
        corrupted[flip_pos] ^= 0xFF;

        let mut cursor = Cursor::new(&corrupted);
        let _result = SKeepAlive::read(&mut cursor, &TEST_VERSION);
    }
}

/// Flip bits in framed packets and verify the TCP decoder doesn't panic.
#[tokio::test]
async fn framed_bit_flips_do_not_cause_panic() {
    let mut frame = Vec::new();
    frame.write_var_int(&VarInt(6)).unwrap();  // packet length
    frame.write_var_int(&VarInt(42)).unwrap(); // keep-alive packet ID
    frame.write_i64_be(0).unwrap();            // payload (keep_alive_id)

    for flip_pos in 0..frame.len() {
        let mut corrupted = frame.clone();
        corrupted[flip_pos] ^= 0xFF;

        let mut decoder = TCPNetworkDecoder::new(Cursor::new(&corrupted));
        let _result = decoder.get_raw_packet().await;
    }
}

// =========================================================================
// 6. Edge-case values for specific fields
// =========================================================================

/// Handshake with boundary port values.
#[test]
fn handshake_port_zero() {
    let mut buf = Vec::new();
    buf.write_var_int(&VarInt(767)).unwrap();
    buf.write_string("localhost").unwrap();
    buf.write_u16_be(0).unwrap();
    buf.write_var_int(&VarInt(2)).unwrap();
    let mut cursor = Cursor::new(&buf);
    let packet = SHandShake::read(&mut cursor, &TEST_VERSION)
        .expect("Port=0 should be valid");
    assert_eq!(packet.server_port, 0);
}

#[test]
fn handshake_port_max() {
    let mut buf = Vec::new();
    buf.write_var_int(&VarInt(767)).unwrap();
    buf.write_string("localhost").unwrap();
    buf.write_u16_be(u16::MAX).unwrap();
    buf.write_var_int(&VarInt(2)).unwrap();
    let mut cursor = Cursor::new(&buf);
    let packet = SHandShake::read(&mut cursor, &TEST_VERSION)
        .expect("Port=u16::MAX should be valid");
    assert_eq!(packet.server_port, u16::MAX);
}

/// Empty string in locale field.
#[test]
fn empty_locale_roundtrip() {
    let mut buf = Vec::new();
    buf.write_string("").unwrap();
    buf.write_i8(12).unwrap();
    buf.write_var_int(&VarInt(0)).unwrap();
    buf.write_bool(true).unwrap();
    buf.write_u8(0x7F).unwrap();
    buf.write_var_int(&VarInt(1)).unwrap();
    buf.write_bool(false).unwrap();
    buf.write_bool(true).unwrap();
    let mut cursor = Cursor::new(&buf);
    let packet = pumpkin_protocol::java::server::play::SClientInformationPlay::read(
        &mut cursor, &TEST_VERSION,
    )
    .expect("Empty locale should be valid");
    assert_eq!(packet.locale, "");
}

/// Nil UUID in login start.
#[test]
fn nil_uuid_login_start() {
    let mut buf = Vec::new();
    buf.write_string("Test").unwrap();
    buf.write_uuid(&uuid::Uuid::nil()).unwrap();
    let mut cursor = Cursor::new(&buf);
    let packet = SLoginStart::read(&mut cursor, &TEST_VERSION)
        .expect("Nil UUID should be valid");
    assert_eq!(packet.uuid, uuid::Uuid::nil());
}
