//! Stable C-ABI types for the native Pumpkin plugin API.
//!
//! Every type in this module is marked `#[repr(C)]` so that native plugins
//! compiled with any Rust version or toolchain can share data across the FFI
//! boundary.  Strings crossing the boundary are represented as `*const c_char`
//! + explicit length pairs; the plugin library owns the backing memory for the
//! lifetime of the query function call unless otherwise noted.

use core::ffi::{c_char, c_uchar, CStr};

// ---------------------------------------------------------------------------
// Versioning
// ---------------------------------------------------------------------------

/// Semantic version used to negotiate API compatibility between the server
/// and a native plugin.
///
/// Compatibility rules (see [`PluginApiVersion::compatible_with`]):
/// - Same `major` → compatible.
/// - Different `major` → incompatible (requires rebuild).
/// - `minor` and `patch` are informational only.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PluginApiVersion {
    /// Breaking-change counter.  Bump when the plugin ABI changes.
    pub major: u32,
    /// Feature-add counter.  Reset to `0` when `major` bumps.
    pub minor: u32,
    /// Bug-fix counter.  Reset to `0` when `major` or `minor` bumps.
    pub patch: u32,
}

impl PluginApiVersion {
    /// Current API version of this server build.
    pub const CURRENT: Self = Self {
        major: 2,
        minor: 0,
        patch: 0,
    };

    /// Returns `true` if `other` is backward-compatible with this version.
    ///
    /// Two versions are compatible when they share the same `major` number.
    #[must_use]
    pub const fn compatible_with(&self, other: &Self) -> bool {
        self.major == other.major
    }

    /// Encode the version as a single `u32` packed as
    /// `(major << 22) | (minor << 12) | patch`.
    /// Useful for compact storage or wire protocols.
    #[must_use]
    pub const fn packed(&self) -> u32 {
        let major = if self.major > 0x3FF { 0x3FF } else { self.major };
        let minor = if self.minor > 0x3FF { 0x3FF } else { self.minor };
        let patch = if self.patch > 0xFFF { 0xFFF } else { self.patch };
        (major << 22) | (minor << 12) | patch
    }
}

impl core::fmt::Display for PluginApiVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ---------------------------------------------------------------------------
// Plugin metadata (C-ABI friendly)
// ---------------------------------------------------------------------------

/// Plugin metadata exposed to the server through the FFI boundary.
///
/// All string fields are **borrowed** from the plugin's static data and are
/// valid for the duration of the `metadata` query function call.  The server
/// must copy any strings it needs to retain.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    /// Human-readable name of the plugin (e.g. `"Essentials"`).
    pub name: *const c_char,
    /// Version string (e.g. `"1.4.2"`).
    pub version: *const c_char,
    /// Comma-separated author list.
    pub authors: *const c_char,
    /// Short description of what the plugin does.
    pub description: *const c_char,
    /// Comma-separated list of plugin dependency names.
    pub dependencies: *const c_char,
    /// Comma-separated list of permissions the plugin requires.
    pub permissions: *const c_char,
    /// The API version this plugin was compiled against.
    pub api_version: PluginApiVersion,
}

// Safety: `PluginMetadata` contains only `*const c_char` pointers (send + sync).
unsafe impl Send for PluginMetadata {}
unsafe impl Sync for PluginMetadata {}

/// Owned representation of [`PluginMetadata`] with heap-allocated strings.
///
/// This is what the server side uses after converting from the C-ABI form.
#[derive(Debug, Clone)]
pub struct OwnedPluginMetadata {
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub description: String,
    pub dependencies: Vec<String>,
    pub permissions: Vec<String>,
    pub api_version: PluginApiVersion,
}

impl PluginMetadata {
    /// Convert the C-ABI metadata into an owned representation.
    ///
    /// # Safety
    /// Every `*const c_char` pointer must be a valid NUL-terminated C string
    /// that remains valid for the duration of this call.
    #[must_use]
    pub unsafe fn to_owned(&self) -> OwnedPluginMetadata {
        OwnedPluginMetadata {
            name: unsafe { self.safe_cstr("name") },
            version: unsafe { self.safe_cstr("version") },
            authors: unsafe { self.safe_cstr("authors") }
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            description: unsafe { self.safe_cstr("description") },
            dependencies: unsafe { self.safe_cstr("dependencies") }
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            permissions: unsafe { self.safe_cstr("permissions") }
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            api_version: self.api_version,
        }
    }

    /// Read a potentially-null C string field, returning an empty string for
    /// null pointers.
    ///
    /// # Safety
    /// The internal pointer for `field` must be valid or null.
    unsafe fn safe_cstr(&self, field: &str) -> String {
        let ptr: *const c_char = match field {
            "name" => self.name,
            "version" => self.version,
            "authors" => self.authors,
            "description" => self.description,
            "dependencies" => self.dependencies,
            "permissions" => self.permissions,
            _ => core::ptr::null(),
        };
        if ptr.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
        }
    }
}

// ---------------------------------------------------------------------------
// Log level (mirrors tracing::Level)
// ---------------------------------------------------------------------------

/// Log severity level used by native plugins.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

// ---------------------------------------------------------------------------
// Event callback result
// ---------------------------------------------------------------------------

/// Codes returned by event callback functions to control event propagation.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventCallbackResult {
    /// Continue normal event processing (pass event to next handler).
    Continue = 0,
    /// Stop processing this event (no further handlers called).
    Stop = 1,
    /// Cancel the event (for cancellable events only).
    Cancel = 2,
}

// ---------------------------------------------------------------------------
// Command sender types
// ---------------------------------------------------------------------------

/// Who executed a command.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSenderKind {
    Player = 0,
    Console = 1,
    CommandBlock = 2,
    Rcon = 3,
}

/// Opaque handle representing the sender of a command.
///
/// The `kind` discriminates the type, while `id` holds a server-internal
/// identifier (e.g. a player's UUID bytes or console marker).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CommandSender {
    pub kind: CommandSenderKind,
    pub id: [u8; 16],
}

// ---------------------------------------------------------------------------
// Helper: owned strings that can be returned across FFI
// ---------------------------------------------------------------------------

/// A NUL-terminated C string allocated by the server and returned to the
/// plugin.  The plugin **must** call [`CStringDrop::drop_string`] when done
/// to avoid leaking memory.
#[repr(C)]
#[derive(Debug)]
pub struct CStringOut {
    pub data: *mut c_char,
    pub len: usize,
}

/// Function pointer type for freeing strings allocated by the server.
pub type CStringDropFn = unsafe extern "C" fn(*mut c_char, usize);

/// Drop helper that the server exports so plugins can free returned strings.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CStringDropVTable {
    pub drop_string: Option<CStringDropFn>,
}

// ---------------------------------------------------------------------------
// Result type for FFI operations
// ---------------------------------------------------------------------------

/// A simple FFI-safe result type.
///
/// `success` indicates whether the operation succeeded.  When it is `false`,
/// the `error_message` pointer (which may be null) can be read for a
/// human-readable description.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct FfiResult {
    pub success: bool,
    pub error_message: *const c_char,
}

impl FfiResult {
    pub const OK: Self = Self {
        success: true,
        error_message: core::ptr::null(),
    };

    /// # Safety
    /// `error_message` must be a valid NUL-terminated C string or null.
    #[must_use]
    pub const fn error(msg: *const c_char) -> Self {
        Self {
            success: false,
            error_message: msg,
        }
    }
}

// ---------------------------------------------------------------------------
// Raw event data container (used in event callbacks)
// ---------------------------------------------------------------------------

/// Opaque identifier for an event type, matching the server's internal
/// event registry.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventTypeId(pub u32);

/// A raw event payload passed through the FFI boundary.
///
/// The `event_type` identifies which concrete event this is.
/// `data` points to a serialized representation of the event's data
/// (e.g. JSON, MessagePack, or a `repr(C)` struct identified by
/// `event_type`).
/// `data_len` is the number of bytes at `data`.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct RawEvent {
    pub event_type: EventTypeId,
    pub data: *const c_uchar,
    pub data_len: usize,
}

// Safety: RawEvent contains only raw pointers but is Send+Sync because
// the data is read-only during the callback and the server owns the backing memory.
unsafe impl Send for RawEvent {}
unsafe impl Sync for RawEvent {}
