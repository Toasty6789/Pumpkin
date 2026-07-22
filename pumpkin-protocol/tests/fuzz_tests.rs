// =============================================================================
// Round-Trip Fuzz Tests — Pumpkin Protocol
// =============================================================================
//
// These tests verify that all major packet types can survive a full
// encode → decode round trip without data loss or corruption.
//
// For serverbound (S*) packets that implement only `ServerPacket::read`, we
// construct raw bytes using the `NetworkWriteExt` helpers, then deserialise
// them and verify every field.  For bidirectional packets (SHandShake,
// SChatMessage) we also exercise the full ClientPacket → ServerPacket path.
//
// Coverage: Handshake, Status, Login, Config, and Play protocol phases.
//
// =============================================================================

use std::io::Cursor;

use pumpkin_protocol::{
    ServerPacket,
    codec::var_int::VarInt,
    java::server::{
        config,
        handshake::SHandShake,
        login,
        play,
        status,
    },
    ser::{NetworkReadExt, NetworkWriteExt},
};
use pumpkin_util::{
    math::{position::BlockPos, vector3::Vector3},
    version::JavaMinecraftVersion,
};

/// The Minecraft version used for all test serialisation.
const V: JavaMinecraftVersion = JavaMinecraftVersion::V_1_21_4;

// ---------------------------------------------------------------------------
// Helper: write known bytes to a Vec, then read them back as the given type.
// ---------------------------------------------------------------------------
fn write_then_read_server<P, F>(version: &JavaMinecraftVersion, writer: F) -> P
where
    P: ServerPacket,
    F: FnOnce(&mut Vec<u8>),
{
    let mut buf = Vec::new();
    writer(&mut buf);
    let mut cursor = Cursor::new(&buf);
    P::read(&mut cursor, version).expect("ServerPacket::read failed")
}

// =========================================================================
// Handshake
// =========================================================================

#[test]
fn handshake_roundtrip() {
    let packet: SHandShake = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(767)).unwrap();          // protocol_version
        buf.write_string("localhost").unwrap();             // server_address
        buf.write_u16_be(25565).unwrap();                   // server_port
        buf.write_var_int(&VarInt(2)).unwrap();             // next_state (Login)
    });
    assert_eq!(packet.protocol_version, VarInt(767));
    assert_eq!(&*packet.server_address, "localhost");
    assert_eq!(packet.server_port, 25565);
    assert_eq!(packet.next_state, pumpkin_protocol::ConnectionState::Login);
}

/// Also test that the bidirectional (ClientPacket + ServerPacket) path works.
#[test]
fn handshake_bidirectional() {
    use pumpkin_protocol::ClientPacket;
    let original = SHandShake {
        protocol_version: VarInt(768),
        server_address: Box::from("mc.example.com"),
        server_port: 25565,
        next_state: pumpkin_protocol::ConnectionState::Status,
    };
    let mut buf = Vec::new();
    original.write_packet_data(&mut buf, &V).expect("write");
    let mut cursor = Cursor::new(&buf);
    let decoded = SHandShake::read(&mut cursor, &V).expect("read");
    assert_eq!(original.protocol_version, decoded.protocol_version);
    assert_eq!(original.server_address, decoded.server_address);
    assert_eq!(original.server_port, decoded.server_port);
}

// =========================================================================
// Status
// =========================================================================

#[test]
fn status_request_decode() {
    let _packet: status::SStatusRequest = write_then_read_server(&V, |_buf| {
        // SStatusRequest has no fields — empty payload
    });
}

#[test]
fn status_ping_request_roundtrip() {
    let packet: status::SStatusPingRequest = write_then_read_server(&V, |buf| {
        buf.write_i64_be(42).unwrap();
    });
    assert_eq!(packet.payload, 42);
}

// =========================================================================
// Login
// =========================================================================

#[test]
fn login_start_roundtrip() {
    let uuid = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let packet: login::SLoginStart = write_then_read_server(&V, |buf| {
        buf.write_string("Player1").unwrap();
        buf.write_uuid(&uuid).unwrap();
    });
    assert_eq!(&*packet.name, "Player1");
    assert_eq!(packet.uuid, uuid);
}

#[test]
fn encryption_response_roundtrip() {
    let packet: login::SEncryptionResponse = write_then_read_server(&V, |buf| {
        let secret: &[u8] = &[0xAB; 128];
        buf.write_var_int(&VarInt(secret.len() as i32)).unwrap();
        buf.write_slice(secret).unwrap();
        let token: &[u8] = &[0xCD; 16];
        buf.write_var_int(&VarInt(token.len() as i32)).unwrap();
        buf.write_slice(token).unwrap();
    });
    assert_eq!(packet.shared_secret.as_ref(), &[0xABu8; 128]);
    assert_eq!(packet.verify_token.as_ref(), &[0xCDu8; 16]);
}

#[test]
fn login_plugin_response_roundtrip() {
    let packet: login::SLoginPluginResponse = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(1)).unwrap();
        buf.write_bool(true).unwrap();
        let payload: &[u8] = &[0x01, 0x02, 0x03];
        buf.write_var_int(&VarInt(payload.len() as i32)).unwrap();
        buf.write_slice(payload).unwrap();
    });
    assert_eq!(packet.message_id, VarInt(1));
    assert_eq!(packet.data.as_deref(), Some(&[0x01u8, 0x02, 0x03][..]));
}

// =========================================================================
// Config
// =========================================================================

#[test]
fn config_client_information_roundtrip() {
    let packet: config::SClientInformationConfig = write_then_read_server(&V, |buf| {
        buf.write_string("en_US").unwrap();
        buf.write_i8(12).unwrap();
        buf.write_var_int(&VarInt(0)).unwrap();   // chat_mode
        buf.write_bool(true).unwrap();             // chat_colors
        buf.write_u8(0x7F).unwrap();               // skin_parts
        buf.write_var_int(&VarInt(1)).unwrap();    // main_hand
        buf.write_bool(false).unwrap();            // text_filtering
        buf.write_bool(true).unwrap();             // server_listing
    });
    assert_eq!(packet.locale, "en_US");
    assert_eq!(packet.view_distance, 12);
    assert_eq!(packet.chat_mode, VarInt(0));
    assert!(packet.chat_colors);
    assert_eq!(packet.skin_parts, 0x7F);
}

#[test]
fn config_plugin_message_roundtrip() {
    let packet: config::SPluginMessage = write_then_read_server(&V, |buf| {
        buf.write_string("minecraft:brand").unwrap();
        let data: &[u8] = &[0x00, 0x05, b'P', b'u', b'm', b'p', b'k', b'i', b'n'];
        buf.write_slice(data).unwrap();
    });
    assert_eq!(&*packet.channel, "minecraft:brand");
    assert_eq!(&*packet.data, &[0x00, 0x05, b'P', b'u', b'm', b'p', b'k', b'i', b'n']);
}

#[test]
fn config_known_packs_roundtrip() {
    let packet: config::SKnownPacks = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(0)).unwrap();
    });
    assert_eq!(packet.known_pack_count, VarInt(0));
}

#[test]
fn config_resource_pack_roundtrip() {
    let uuid = uuid::Uuid::nil();
    let packet: config::SConfigResourcePack = write_then_read_server(&V, |buf| {
        buf.write_uuid(&uuid).unwrap();
        buf.write_var_int(&VarInt(3)).unwrap(); // Accepted
    });
    assert_eq!(packet.uuid, uuid);
    assert_eq!(packet.result, VarInt(3));
}

#[test]
fn config_keep_alive_roundtrip() {
    let packet: config::SKeepAlive = write_then_read_server(&V, |buf| {
        buf.write_i64_be(123456).unwrap();
    });
    assert_eq!(packet.keep_alive_id, 123456);
}

// =========================================================================
// Play — keep alive
// =========================================================================

#[test]
fn play_keep_alive_roundtrip() {
    let packet: play::SKeepAlive = write_then_read_server(&V, |buf| {
        buf.write_i64_be(999).unwrap();
    });
    assert_eq!(packet.keep_alive_id, 999);
}

#[test]
fn play_confirm_teleport_roundtrip() {
    let packet: play::SConfirmTeleport = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(42)).unwrap();
    });
    assert_eq!(packet.teleport_id, VarInt(42));
}

#[test]
fn play_chat_command_roundtrip() {
    let packet: play::SChatCommand = write_then_read_server(&V, |buf| {
        buf.write_string("/help").unwrap();
    });
    assert_eq!(packet.command, "/help");
}

#[test]
fn play_client_information_roundtrip() {
    let packet: play::SClientInformationPlay = write_then_read_server(&V, |buf| {
        buf.write_string("en_US").unwrap();
        buf.write_i8(12).unwrap();
        buf.write_var_int(&VarInt(0)).unwrap();
        buf.write_bool(true).unwrap();
        buf.write_u8(0x7F).unwrap();
        buf.write_var_int(&VarInt(1)).unwrap();
        buf.write_bool(false).unwrap();
        buf.write_bool(true).unwrap();
    });
    assert_eq!(packet.locale, "en_US");
    assert_eq!(packet.view_distance, 12);
    assert_eq!(packet.chat_mode, VarInt(0));
    assert!(packet.chat_colors);
}

#[test]
fn play_player_position_roundtrip() {
    let packet: play::SPlayerPosition = write_then_read_server(&V, |buf| {
        buf.write_f64_be(10.5).unwrap();
        buf.write_f64_be(64.0).unwrap();
        buf.write_f64_be(-20.3).unwrap();
        buf.write_u8(0).unwrap();
    });
    assert_eq!(packet.position.x, 10.5);
    assert_eq!(packet.position.y, 64.0);
    assert_eq!(packet.position.z, -20.3);
    assert_eq!(packet.collision, 0);
}

#[test]
fn play_player_rotation_roundtrip() {
    let packet: play::SPlayerRotation = write_then_read_server(&V, |buf| {
        buf.write_f32_be(90.0).unwrap();
        buf.write_f32_be(45.0).unwrap();
        buf.write_u8(0).unwrap();
    });
    assert_eq!(packet.yaw, 90.0);
    assert_eq!(packet.pitch, 45.0);
    assert_eq!(packet.collision, 0);
}

#[test]
fn play_player_position_rotation_roundtrip() {
    let packet: play::SPlayerPositionRotation = write_then_read_server(&V, |buf| {
        buf.write_f64_be(0.0).unwrap();
        buf.write_f64_be(64.0).unwrap();
        buf.write_f64_be(0.0).unwrap();
        buf.write_f32_be(180.0).unwrap();
        buf.write_f32_be(0.0).unwrap();
        buf.write_u8(0).unwrap();
    });
    assert_eq!(packet.position.x, 0.0);
    assert_eq!(packet.yaw, 180.0);
}

#[test]
fn play_set_player_ground_roundtrip() {
    let packet: play::SSetPlayerGround = write_then_read_server(&V, |buf| {
        buf.write_bool(true).unwrap();
        buf.write_u8(0).unwrap();
    });
    assert!(packet.on_ground);
    assert_eq!(packet.collision, 0);
}

#[test]
fn play_player_action_roundtrip() {
    let packet: play::SPlayerAction = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(0)).unwrap();
        buf.write_i64_be(BlockPos::new(10, 64, 10).to_i64()).unwrap();
        buf.write_u8(1).unwrap();
        buf.write_var_int(&VarInt(0)).unwrap();
    });
    assert_eq!(packet.status, VarInt(0));
    assert_eq!(packet.face, 1);
}

#[test]
fn play_swing_arm_roundtrip() {
    let packet: play::SSwingArm = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(0)).unwrap();
    });
    assert_eq!(packet.hand, VarInt(0));
}

#[test]
fn play_use_item_on_roundtrip() {
    let packet: play::SUseItemOn = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(0)).unwrap();       // hand
        buf.write_i64_be(BlockPos::new(100, 64, -100).to_i64()).unwrap(); // position
        buf.write_var_int(&VarInt(1)).unwrap();       // face
        buf.write_f32_be(0.5).unwrap();               // cursor_x
        buf.write_f32_be(0.5).unwrap();               // cursor_y
        buf.write_f32_be(0.5).unwrap();               // cursor_z
        buf.write_bool(false).unwrap();               // inside_block
        buf.write_bool(false).unwrap();               // is_against_world_border
        buf.write_var_int(&VarInt(1)).unwrap();       // sequence
    });
    assert_eq!(packet.hand, VarInt(0));
    assert_eq!(packet.face, VarInt(1));
}

#[test]
fn play_use_item_roundtrip() {
    let packet: play::SUseItem = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(0)).unwrap();
        buf.write_var_int(&VarInt(0)).unwrap();
        buf.write_f32_be(0.0).unwrap(); // yaw
        buf.write_f32_be(0.0).unwrap(); // pitch
    });
    assert_eq!(packet.hand, VarInt(0));
}

#[test]
fn play_interact_roundtrip() {
    let packet: play::SInteract = write_then_read_server(&V, |buf| {
        // V_1_21_4 uses the old format (type field + f32 position)
        buf.write_var_int(&VarInt(42)).unwrap();    // entity_id
        buf.write_var_int(&VarInt(2)).unwrap();     // type: InteractAt
        buf.write_f32_be(1.5).unwrap();             // target_x
        buf.write_f32_be(2.5).unwrap();             // target_y
        buf.write_f32_be(3.5).unwrap();             // target_z
        buf.write_var_int(&VarInt(0)).unwrap();     // hand
        buf.write_bool(false).unwrap();             // sneaking
    });
    assert_eq!(packet.entity_id, VarInt(42));
}

#[test]
fn play_player_command_roundtrip() {
    let packet: play::SPlayerCommand = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(0)).unwrap();     // entity_id
        buf.write_var_int(&VarInt(0)).unwrap();     // action: StartSneaking
        buf.write_var_int(&VarInt(0)).unwrap();     // jump_boost
    });
    assert_eq!(packet.entity_id, VarInt(0));
}

#[test]
fn play_player_input_roundtrip() {
    let packet: play::SPlayerInput = write_then_read_server(&V, |buf| {
        buf.write_i8(play::SPlayerInput::FORWARD | play::SPlayerInput::JUMP).unwrap();
    });
    assert_eq!(packet.input, play::SPlayerInput::FORWARD | play::SPlayerInput::JUMP);
}

#[test]
fn play_set_held_item_roundtrip() {
    let packet: play::SSetHeldItem = write_then_read_server(&V, |buf| {
        buf.write_i16_be(4).unwrap();
    });
    assert_eq!(packet.slot, 4);
}

#[test]
fn play_set_creative_slot_roundtrip() {
    let packet: play::SSetCreativeSlot = write_then_read_server(&V, |buf| {
        buf.write_i16_be(36).unwrap();
        buf.write_bool(false).unwrap(); // no item
    });
    assert_eq!(packet.slot, 36);
}

#[test]
fn play_close_container_roundtrip() {
    let packet: play::SCloseContainer = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(0)).unwrap();
    });
    assert_eq!(packet.window_id, VarInt(0));
}

#[test]
fn play_change_game_mode_roundtrip() {
    let packet: play::SChangeGameMode = write_then_read_server(&V, |buf| {
        buf.write_u8(1).unwrap(); // Survival
    });
    use pumpkin_util::GameMode;
    assert_eq!(packet.game_mode, GameMode::Survival);
}

#[test]
fn play_paddle_boat_roundtrip() {
    let packet: play::SPaddleBoat = write_then_read_server(&V, |buf| {
        buf.write_bool(true).unwrap();
        buf.write_bool(false).unwrap();
    });
    assert!(packet.left_paddle);
    assert!(!packet.right_paddle);
}

#[test]
fn play_move_vehicle_roundtrip() {
    let packet: play::SMoveVehicle = write_then_read_server(&V, |buf| {
        buf.write_f64_be(0.0).unwrap();
        buf.write_f64_be(64.0).unwrap();
        buf.write_f64_be(0.0).unwrap();
        buf.write_f32_be(0.0).unwrap();
        buf.write_f32_be(0.0).unwrap();
    });
    assert_eq!(packet.y, 64.0);
}

#[test]
fn play_player_abilities_roundtrip() {
    let packet: play::SPlayerAbilities = write_then_read_server(&V, |buf| {
        buf.write_i8(0x02).unwrap(); // flying flag
    });
    assert_eq!(packet.flags, 0x02);
}

#[test]
fn play_pick_item_from_block_roundtrip() {
    let packet: play::SPickItemFromBlock = write_then_read_server(&V, |buf| {
        buf.write_i64_be(BlockPos::new(0, 0, 0).to_i64()).unwrap();
        buf.write_bool(true).unwrap();
    });
    assert!(packet.include_data);
}

#[test]
fn play_ping_request_roundtrip() {
    let packet: play::SPlayPingRequest = write_then_read_server(&V, |buf| {
        buf.write_i64_be(42).unwrap();
    });
    assert_eq!(packet.payload, 42);
}

#[test]
fn play_player_session_roundtrip() {
    let uuid = uuid::Uuid::nil();
    let packet: play::SPlayerSession = write_then_read_server(&V, |buf| {
        buf.write_uuid(&uuid).unwrap();
        buf.write_i64_be(1_234_567_890).unwrap();
        buf.write_var_int(&VarInt(32)).unwrap();
        buf.write_slice(&[0xAAu8; 32]).unwrap();
        buf.write_var_int(&VarInt(256)).unwrap();
        buf.write_slice(&[0xBBu8; 256]).unwrap();
    });
    assert_eq!(packet.session_id, uuid);
    assert_eq!(packet.expires_at, 1_234_567_890);
    assert_eq!(packet.public_key.len(), 32);
    assert_eq!(packet.key_signature.len(), 256);
}

#[test]
fn play_custom_payload_roundtrip() {
    let packet: play::SCustomPayload = write_then_read_server(&V, |buf| {
        buf.write_string("minecraft:register").unwrap();
        let data: &[u8] = &[0x00];
        buf.write_slice(data).unwrap();
    });
    assert_eq!(&*packet.channel, "minecraft:register");
    assert_eq!(&*packet.data, &[0x00u8]);
}

#[test]
fn play_client_command_roundtrip() {
    let packet: play::SClientCommand = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(0)).unwrap();
    });
    assert_eq!(packet.action_id, VarInt(0));
}

#[test]
fn play_chunk_batch_roundtrip() {
    let packet: play::SChunkBatch = write_then_read_server(&V, |buf| {
        buf.write_f32_be(10.0).unwrap();
    });
    assert_eq!(packet.chunks_per_tick, 10.0);
}

#[test]
fn play_cookie_response_roundtrip() {
    let packet: play::SCookieResponse = write_then_read_server(&V, |buf| {
        buf.write_string("minecraft:test_cookie").unwrap();
        buf.write_bool(false).unwrap(); // no payload
    });
    assert_eq!(&*packet.key, "minecraft:test_cookie");
    assert!(packet.payload.is_none());
}

#[test]
fn play_cookie_response_with_payload_roundtrip() {
    let packet: play::SCookieResponse = write_then_read_server(&V, |buf| {
        buf.write_string("session").unwrap();
        buf.write_bool(true).unwrap();
        let payload: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF];
        buf.write_var_int(&VarInt(payload.len() as i32)).unwrap();
        buf.write_slice(payload).unwrap();
    });
    assert_eq!(&*packet.key, "session");
    assert_eq!(packet.payload.as_deref(), Some(&[0xDE, 0xAD, 0xBE, 0xEF][..]));
}

// =========================================================================
// Edge cases / extreme values
// =========================================================================

#[test]
fn very_long_locale() {
    let packet: play::SClientInformationPlay = write_then_read_server(&V, |buf| {
        let long = "e".repeat(250);
        buf.write_string(&long).unwrap();
        buf.write_i8(12).unwrap();
        buf.write_var_int(&VarInt(0)).unwrap();
        buf.write_bool(true).unwrap();
        buf.write_u8(0x7F).unwrap();
        buf.write_var_int(&VarInt(1)).unwrap();
        buf.write_bool(false).unwrap();
        buf.write_bool(true).unwrap();
    });
    assert_eq!(packet.locale.len(), 250);
}

#[test]
fn zero_values_roundtrip() {
    let packet: play::SPlayerPosition = write_then_read_server(&V, |buf| {
        buf.write_f64_be(0.0).unwrap();
        buf.write_f64_be(0.0).unwrap();
        buf.write_f64_be(0.0).unwrap();
        buf.write_u8(0).unwrap();
    });
    assert_eq!(packet.position.x, 0.0);
}

#[test]
fn max_teleport_id() {
    let packet: play::SConfirmTeleport = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(i32::MAX)).unwrap();
    });
    assert_eq!(packet.teleport_id, VarInt(i32::MAX));
}

#[test]
fn status_request_no_data() {
    // Empty server status request
    let _packet: status::SStatusRequest = write_then_read_server(&V, |_buf| {});
}

#[test]
fn empty_locale() {
    let packet: play::SClientInformationPlay = write_then_read_server(&V, |buf| {
        buf.write_string("").unwrap();
        buf.write_i8(12).unwrap();
        buf.write_var_int(&VarInt(0)).unwrap();
        buf.write_bool(true).unwrap();
        buf.write_u8(0x7F).unwrap();
        buf.write_var_int(&VarInt(1)).unwrap();
        buf.write_bool(false).unwrap();
        buf.write_bool(true).unwrap();
    });
    assert_eq!(packet.locale, "");
}

#[test]
fn port_zero() {
    let _packet: SHandShake = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(767)).unwrap();
        buf.write_string("localhost").unwrap();
        buf.write_u16_be(0).unwrap();
        buf.write_var_int(&VarInt(2)).unwrap();
    });
}

#[test]
fn port_max() {
    let packet: SHandShake = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(767)).unwrap();
        buf.write_string("localhost").unwrap();
        buf.write_u16_be(u16::MAX).unwrap();
        buf.write_var_int(&VarInt(2)).unwrap();
    });
    assert_eq!(packet.server_port, u16::MAX);
}

#[test]
fn long_handshake_address() {
    let packet: SHandShake = write_then_read_server(&V, |buf| {
        buf.write_var_int(&VarInt(767)).unwrap();
        let long_addr = "a".repeat(255);
        buf.write_string(&long_addr).unwrap();
        buf.write_u16_be(25565).unwrap();
        buf.write_var_int(&VarInt(2)).unwrap();
    });
    assert_eq!(packet.server_address.len(), 255);
}
