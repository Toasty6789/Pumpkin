//! Plugin virtual-table (`PluginVTable`) — the core C-ABI contract between the
//! server and every native plugin.
//!
//! A native plugin exposes a `#[no_mangle]` symbol called
//! `PUMPKIN_PLUGIN_VTABLE` that the server resolves with `libloading`.  The
//! vtable is a struct of function pointers, each covering a lifecycle or
//! introspection hook.
//!
//! Version shielding is additionally provided via a separate packed-`u32`
//! symbol `PUMPKIN_API_VERSION` that the server checks before reading the
//! vtable, ensuring backward compatibility at the same `major` version.

use core::ffi::c_void;

use crate::native::events::EventRegistration;
use crate::native::types::{
    CStringDropVTable, EventCallbackResult, LogLevel, PluginApiVersion, PluginMetadata,
};

// ---------------------------------------------------------------------------
// VTable
// ---------------------------------------------------------------------------

/// The primary C-ABI contract that every native Pumpkin plugin must expose.
///
/// # Safety
///
/// All function pointer fields must be non-null for the plugin to be fully
/// functional; the server validates this at load time.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PluginVTable {
    // -- identity / introspection -------------------------------------------
    /// Semantic version of the plugin API this plugin was compiled against.
    pub api_version: PluginApiVersion,

    /// A borrowed metadata descriptor.  The pointed-to data lives in the
    /// plugin's `.rodata` and is valid for the lifetime of the library.
    pub metadata: *const PluginMetadata,

    // -- lifecycle hooks ----------------------------------------------------
    /// Called once when the plugin is first loaded.
    ///
    /// The plugin should perform one-time initialisation (register commands,
    /// spawn background tasks, etc.).  Return an opaque user-data pointer that
    /// will be passed to [`tick`] and [`shutdown`].
    pub init: Option<
        unsafe extern "C" fn(
            server_context: *const c_void,
            server_context_len: usize,
        ) -> *mut c_void,
    >,

    /// Called every server tick (20 times per second by default).
    ///
    /// `user_data` is the pointer returned by [`init`].
    pub tick: Option<unsafe extern "C" fn(user_data: *mut c_void, tick: u64, delta_ms: f64)>,

    /// Called when the plugin is being unloaded or the server is shutting down.
    ///
    /// The plugin should release all resources.  The pointer returned by
    /// [`init`] is passed back so implementations can clean up their state.
    pub shutdown:
        Option<unsafe extern "C" fn(user_data: *mut c_void, reason: *const core::ffi::c_char)>,

    // -- event system -------------------------------------------------------
    /// Return an array of [`EventRegistration`]s describing which events this
    /// plugin wants to listen to.
    ///
    /// `out_count` is written with the number of registrations.
    /// The server takes ownership of the returned array and will free it via
    /// the [`free_event_registrations`] callback.
    pub get_event_registrations:
        Option<unsafe extern "C" fn(out_count: *mut usize) -> *const EventRegistration>,

    /// Free the registrations array returned by [`get_event_registrations`].
    pub free_event_registrations:
        Option<unsafe extern "C" fn(registrations: *const EventRegistration, count: usize)>,

    /// Handle an event that this plugin registered for.
    ///
    /// Return [`EventCallbackResult::Continue`] to allow the event to
    /// propagate to other handlers, or [`EventCallbackResult::Stop`] /
    /// [`EventCallbackResult::Cancel`] to halt processing.
    pub handle_event: Option<
        unsafe extern "C" fn(
            event_id: u32,
            data: *const core::ffi::c_uchar,
            data_len: usize,
            user_data: *mut c_void,
        ) -> EventCallbackResult,
    >,

    // -- command system -----------------------------------------------------
    /// Return the number of commands this plugin registers.
    pub get_command_count: Option<unsafe extern "C" fn() -> u32>,

    // -- logging ------------------------------------------------------------
    /// Log a message through the plugin's own logging facility.
    pub log: Option<
        unsafe extern "C" fn(
            level: LogLevel,
            message: *const core::ffi::c_char,
            user_data: *mut c_void,
        ),
    >,

    // -- string management --------------------------------------------------
    /// Optional vtable for freeing strings allocated by the server.
    pub string_drop: Option<CStringDropVTable>,
}

// Safety: all pointer fields are function pointers or read-only data pointers.
unsafe impl Send for PluginVTable {}
unsafe impl Sync for PluginVTable {}

impl PluginVTable {
    /// Validate the vtable — all required slots must be non-null.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.metadata.is_null()
    }

    /// Check whether this vtable is compatible with a given server version.
    #[must_use]
    pub fn is_compatible_with(&self, server_version: &PluginApiVersion) -> bool {
        server_version.compatible_with(&self.api_version)
    }

    /// # Safety
    ///
    /// The caller must ensure `metadata` points to a valid [`PluginMetadata`].
    #[must_use]
    pub unsafe fn metadata(&self) -> &PluginMetadata {
        unsafe { &*self.metadata }
    }
}

// ---------------------------------------------------------------------------
// Macros for plugin authors
// ---------------------------------------------------------------------------

/// Declare the entry points for a native Pumpkin plugin.
///
/// Generates three `#[no_mangle]` symbols:
///
/// | Symbol | Type | Description |
/// |--------|------|-------------|
/// | `PUMPKIN_API_VERSION` | `u32` | Packed version `(major<<22)` for backward compat |
/// | `PUMPKIN_PLUGIN_METADATA` | `PluginMetadata` | Static metadata (embedded in `.rodata`) |
/// | `PUMPKIN_PLUGIN_VTABLE` | `extern "C" fn() -> *const PluginVTable` | Vtable accessor |
///
/// # Example
///
/// ```rust,ignore
/// use pumpkin_plugin_api::native::prelude::*;
///
/// struct MyPlugin;
///
/// impl NativePlugin for MyPlugin {
///     fn metadata() -> PluginMetadata { /* ... */ }
/// }
///
/// declare_native_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! declare_native_plugin {
    ($plugin:ty) => {
        // Packed version symbol for backward-compatible version checking.
        // Format: (major << 22) | (minor << 12) | patch
        #[no_mangle]
        pub static PUMPKIN_API_VERSION: u32 = {
            const V: $crate::native::types::PluginApiVersion =
                $crate::native::types::PluginApiVersion::CURRENT;
            V.packed()
        };

        // Static metadata instance.
        #[no_mangle]
        pub static PUMPKIN_PLUGIN_METADATA: $crate::native::types::PluginMetadata =
            <$plugin as $crate::native::NativePlugin>::ffi_metadata();

        // Vtable instance (static duration).
        static PLUGIN_VTABLE: $crate::native::vtable::PluginVTable =
            <$plugin as $crate::native::NativePlugin>::vtable();

        // Vtable accessor function.
        #[no_mangle]
        pub unsafe extern "C" fn PUMPKIN_PLUGIN_VTABLE()
        -> *const $crate::native::vtable::PluginVTable {
            &PLUGIN_VTABLE as *const $crate::native::vtable::PluginVTable
        }
    };
}

// ---------------------------------------------------------------------------
// NativePlugin trait
// ---------------------------------------------------------------------------

/// Trait that every native Pumpkin plugin must implement.
///
/// Use the [`declare_native_plugin!`] macro to generate the C-ABI exports.
pub trait NativePlugin: Send + Sync + 'static {
    /// Return the plugin's metadata (static data — no allocations).
    fn metadata() -> crate::native::types::PluginMetadata;

    /// Called when the plugin is loaded.
    ///
    /// Return an opaque user-data pointer that will be passed to [`on_shutdown`]
    /// and [`on_tick`].  Return `core::ptr::null_mut()` if not needed.
    fn on_init() -> *mut core::ffi::c_void {
        core::ptr::null_mut()
    }

    /// Called every tick.
    fn on_tick(_user_data: *mut c_void, _tick: u64, _delta_ms: f64) {}

    /// Called when the plugin is unloaded.
    fn on_shutdown(_user_data: *mut c_void, _reason: *const core::ffi::c_char) {}

    /// Return the events this plugin wants to listen to.
    fn get_event_registrations() -> &'static [EventRegistration] {
        &[]
    }

    /// Handle an event.
    fn handle_event(
        _event_id: u32,
        _data: *const core::ffi::c_uchar,
        _data_len: usize,
    ) -> EventCallbackResult {
        EventCallbackResult::Continue
    }

    /// Number of commands registered by this plugin.
    fn command_count() -> u32 {
        0
    }

    // -- internal helpers (used by declare_native_plugin!) ------------------

    #[doc(hidden)]
    fn ffi_metadata() -> PluginMetadata {
        Self::metadata()
    }

    #[doc(hidden)]
    fn vtable() -> PluginVTable {
        PluginVTable {
            api_version: PluginApiVersion::CURRENT,
            metadata: &Self::ffi_metadata() as *const PluginMetadata,
            init: Some(Self::ffi_init),
            tick: Some(Self::ffi_tick),
            shutdown: Some(Self::ffi_shutdown),
            get_event_registrations: Some(Self::ffi_get_event_registrations),
            free_event_registrations: Some(Self::ffi_free_event_registrations),
            handle_event: Some(Self::ffi_handle_event),
            get_command_count: Some(Self::ffi_command_count),
            log: None,
            string_drop: None,
        }
    }

    #[doc(hidden)]
    unsafe extern "C" fn ffi_init(
        _server_context: *const c_void,
        _server_context_len: usize,
    ) -> *mut c_void {
        Self::on_init()
    }

    #[doc(hidden)]
    unsafe extern "C" fn ffi_tick(user_data: *mut c_void, tick: u64, delta_ms: f64) {
        Self::on_tick(user_data, tick, delta_ms);
    }

    #[doc(hidden)]
    unsafe extern "C" fn ffi_shutdown(user_data: *mut c_void, reason: *const core::ffi::c_char) {
        Self::on_shutdown(user_data, reason);
    }

    #[doc(hidden)]
    unsafe extern "C" fn ffi_get_event_registrations(
        out_count: *mut usize,
    ) -> *const EventRegistration {
        let regs = Self::get_event_registrations();
        unsafe {
            *out_count = regs.len();
        }
        regs.as_ptr()
    }

    #[doc(hidden)]
    unsafe extern "C" fn ffi_free_event_registrations(
        _registrations: *const EventRegistration,
        _count: usize,
    ) {
        // Static slice — nothing to free.
    }

    #[doc(hidden)]
    unsafe extern "C" fn ffi_handle_event(
        event_id: u32,
        data: *const core::ffi::c_uchar,
        data_len: usize,
        _user_data: *mut core::ffi::c_void,
    ) -> EventCallbackResult {
        Self::handle_event(event_id, data, data_len)
    }

    #[doc(hidden)]
    unsafe extern "C" fn ffi_command_count() -> u32 {
        Self::command_count()
    }
}
