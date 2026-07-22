//! FFI-safe `#[repr(C)]` types for Pumpkin plugin events.
//!
//! These types mirror the server-side event system so that native plugins can
//! receive and respond to events across the C-ABI boundary without requiring
//! the same Rust compiler or standard library version.

use core::ffi::c_uchar;

use crate::native::types::EventCallbackResult;

// ---------------------------------------------------------------------------
// Event callback function signature
// ---------------------------------------------------------------------------

/// Signature of a native plugin's event handler callback.
///
/// # Parameters
/// - `event_id`: Unique identifier of the event type being fired.
/// - `event`: Pointer to the raw event data.  The exact structure depends on
///   `event_id`; see the event registry for the mapping.
/// - `event_len`: Number of bytes available at `event`.
/// - `user_data`: Opaque pointer provided when the callback was registered.
///
/// # Returns
/// An [`EventCallbackResult`] instructing the server how to proceed.
pub type EventCallback = unsafe extern "C" fn(
    event_id: u32,
    event: *const c_uchar,
    event_len: usize,
    user_data: *mut core::ffi::c_void,
) -> EventCallbackResult;

// ---------------------------------------------------------------------------
// Registration entry for a single event callback
// ---------------------------------------------------------------------------

/// Describes a single event subscription that a native plugin wants to
/// register with the server.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct EventRegistration {
    /// Server-internal event type id.
    pub event_id: u32,
    /// Callback function pointer.
    pub callback: Option<EventCallback>,
    /// Opaque data forwarded to the callback on every invocation.
    pub user_data: *mut core::ffi::c_void,
    /// Priority (lower values run first).
    pub priority: i32,
    /// Whether this handler runs in a blocking (synchronous) fashion.
    pub blocking: bool,
}

// Safety: the raw pointer is only forwarded to the callback.
unsafe impl Send for EventRegistration {}
unsafe impl Sync for EventRegistration {}

// ---------------------------------------------------------------------------
// Pre-defined event type IDs (must match server-side EVENT_REGISTRY)
// ---------------------------------------------------------------------------

/// Well-known event type identifiers shared between server and plugins.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WellKnownEventId {
    // -- lifecycle --
    ServerStartup = 1,
    ServerTick = 2,
    ServerShutdown = 3,

    // -- player --
    PlayerJoin = 10,
    PlayerQuit = 11,
    PlayerChat = 12,
    PlayerDeath = 13,
    PlayerRespawn = 14,
    PlayerMove = 15,

    // -- block --
    BlockBreak = 20,
    BlockPlace = 21,
    BlockInteract = 22,

    // -- packets --
    PacketReceived = 30,
    PacketSent = 31,

    // -- world --
    WorldLoad = 40,
    WorldSave = 41,

    // Reserved range for user / third-party events
    CustomStart = 1024,
}

// ---------------------------------------------------------------------------
// Player chat event (example serializable FFI event)
// ---------------------------------------------------------------------------

/// Data for the [`WellKnownEventId::PlayerChat`] event.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PlayerChatEventData {
    /// 16-byte UUID of the player who sent the message.
    pub player_uuid: [u8; 16],
    /// NUL-terminated message content.  May be modified in-place for
    /// blocking handlers.
    pub message: [i8; 256],
    /// Whether this event has been cancelled.
    pub cancelled: bool,
}

// ---------------------------------------------------------------------------
// Player join event
// ---------------------------------------------------------------------------

/// Data for the [`WellKnownEventId::PlayerJoin`] event.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PlayerJoinEventData {
    /// 16-byte UUID of the joining player.
    pub player_uuid: [u8; 16],
    /// NUL-terminated player name.
    pub player_name: [i8; 64],
}

// ---------------------------------------------------------------------------
// Block break event
// ---------------------------------------------------------------------------

/// Data for the [`WellKnownEventId::BlockBreak`] event.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BlockBreakEventData {
    pub player_uuid: [u8; 16],
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Minecraft resource location of the block (e.g. `"minecraft:stone"`).
    pub block_id: [i8; 128],
    pub cancelled: bool,
}

// ---------------------------------------------------------------------------
// Server tick event
// ---------------------------------------------------------------------------

/// Data for the [`WellKnownEventId::ServerTick`] event.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ServerTickEventData {
    /// Tick number since server start.
    pub tick: u64,
    /// Delta time in milliseconds since the last tick.
    pub delta_ms: f64,
}
