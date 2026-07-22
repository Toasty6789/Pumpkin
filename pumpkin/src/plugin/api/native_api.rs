//! Server-side integration for the native (C-ABI) plugin API.
//!
//! Provides [`NativePluginHandle`] — a safe wrapper around a dynamically loaded
//! native plugin library that validates API version compatibility, resolves
//! the [`PluginVTable`](pumpkin_plugin_api::native::vtable::PluginVTable),
//! and mediates all lifecycle and event callbacks.
//!
//! This module is consumed by [`super::super::loader::native`].

use std::ffi::CStr;
use std::sync::Arc;

use libloading::Library;
use tracing::{debug, info};

use pumpkin_plugin_api::native::events::{EventCallback, EventRegistration};
use pumpkin_plugin_api::native::types::{
    CStringDropVTable, EventCallbackResult, LogLevel, OwnedPluginMetadata, PluginApiVersion,
};
use pumpkin_plugin_api::native::vtable::PluginVTable;

use super::PluginMetadata;
use crate::plugin::PLUGIN_API_VERSION;

// ---------------------------------------------------------------------------
// Safe handle wrapping a loaded native plugin library + its vtable
// ---------------------------------------------------------------------------

/// A loaded native plugin, holding the dynamic library and its validated vtable.
pub struct NativePluginHandle {
    /// The underlying dynamic library — kept alive for the plugin's lifetime.
    pub(crate) library: Arc<Library>,
    /// The resolved vtable (static reference — valid as long as `library` lives).
    pub(crate) vtable: &'static PluginVTable,
    /// Plugin metadata (owned copy, converted from C-ABI).
    pub(crate) metadata: PluginMetadata,
    /// Opaque state pointer returned by the plugin's `init` hook.
    pub(crate) user_data: *mut std::ffi::c_void,
    /// Optional string free vtable from the plugin.
    pub(crate) string_drop: Option<CStringDropVTable>,
    /// Registered event registrations (owned copies).
    pub(crate) event_registrations: Vec<EventRegistration>,
}

// Safety: `user_data` is only accessed through the vtable functions.
unsafe impl Send for NativePluginHandle {}
unsafe impl Sync for NativePluginHandle {}

impl NativePluginHandle {
    /// Attempt to load a native plugin from a dynamic library.
    ///
    /// # Errors
    ///
    /// Returns an error string if the library cannot be opened, the required
    /// symbols are missing, or the API version is incompatible.
    ///
    /// # Safety
    ///
    /// The caller must ensure `library` has not yet been inspected for the
    /// symbols accessed here.
    pub unsafe fn load(library: Arc<Library>) -> Result<Self, String> {
        // 1. Resolve the legacy PUMPKIN_API_VERSION symbol (packed u32).
        let plugin_api_version_raw: u32 = unsafe {
            match library.get::<*const u32>(b"PUMPKIN_API_VERSION\0") {
                Ok(sym) => **sym,
                Err(_) => {
                    return Err(
                        "Missing PUMPKIN_API_VERSION symbol — plugin was not built for this server"
                            .into(),
                    );
                }
            }
        };

        // 2. Unpack the version (same packing: major<<22 | minor<<12 | patch).
        let plugin_api_version = PluginApiVersion {
            major: (plugin_api_version_raw >> 22) & 0x3FF,
            minor: (plugin_api_version_raw >> 12) & 0x3FF,
            patch: plugin_api_version_raw & 0xFFF,
        };

        let server_api_version = PluginApiVersion {
            major: (PLUGIN_API_VERSION >> 22) & 0x3FF,
            minor: (PLUGIN_API_VERSION >> 12) & 0x3FF,
            patch: PLUGIN_API_VERSION & 0xFFF,
        };

        // 3. Validate compatibility (same major = compatible).
        if !server_api_version.compatible_with(&plugin_api_version) {
            return Err(format!(
                "Plugin API version mismatch: plugin v{}, server v{} \
                 (major versions differ — rebuild plugin against this server)",
                plugin_api_version, server_api_version,
            ));
        }

        // 4. Resolve the PUMPKIN_PLUGIN_VTABLE symbol.
        let vtable_ptr: *const PluginVTable = unsafe {
            match library.get::<*const PluginVTable>(b"PUMPKIN_PLUGIN_VTABLE\0") {
                Ok(sym) => *sym,
                Err(_) => {
                    return Err(
                        "Missing PUMPKIN_PLUGIN_VTABLE symbol — plugin does not expose a valid vtable"
                            .into(),
                    );
                }
            }
        };

        if vtable_ptr.is_null() {
            return Err("PUMPKIN_PLUGIN_VTABLE symbol resolved to null".into());
        }

        let vtable: &PluginVTable = unsafe { &*vtable_ptr };

        if !vtable.is_valid() {
            return Err("Plugin vtable validation failed (null metadata pointer)".into());
        }

        // 5. Convert C-ABI metadata to server's internal PluginMetadata.
        let metadata_raw = unsafe { vtable.metadata() };
        let owned = unsafe { metadata_raw.try_to_owned() }
            .map_err(|error| format!("Invalid native plugin metadata: {error}"))?;
        let metadata = PluginMetadata {
            name: owned.name,
            version: owned.version,
            authors: owned.authors,
            description: owned.description,
            dependencies: owned.dependencies,
            permissions: owned.permissions,
        };

        // 6. Call the init hook if present.
        let user_data = if let Some(init_fn) = vtable.init {
            unsafe { init_fn(std::ptr::null(), 0) }
        } else {
            std::ptr::null_mut()
        };

        // 7. Collect event registrations.
        let event_registrations = Self::collect_event_registrations(vtable);

        // 8. Optional string drop vtable.
        let string_drop = vtable.string_drop;

        info!(
            "Loaded native plugin '{}' v{} (API {}.{}.{})",
            metadata.name,
            metadata.version,
            plugin_api_version.major,
            plugin_api_version.minor,
            plugin_api_version.patch,
        );

        Ok(Self {
            library,
            vtable,
            metadata,
            user_data,
            string_drop,
            event_registrations,
        })
    }

    /// Collect event registrations from the vtable.
    fn collect_event_registrations(vtable: &PluginVTable) -> Vec<EventRegistration> {
        let Some(get_regs) = vtable.get_event_registrations else {
            return Vec::new();
        };

        unsafe {
            let mut count: usize = 0;
            let ptr = get_regs(&mut count as *mut usize);
            if ptr.is_null() || count == 0 {
                return Vec::new();
            }

            let slice = std::slice::from_raw_parts(ptr, count);
            let registrations = slice.to_vec();

            // Let the plugin free its array.
            if let Some(free_fn) = vtable.free_event_registrations {
                free_fn(ptr, count);
            }

            registrations
        }
    }

    /// Run the plugin's tick hook.
    pub fn tick(&self, tick_num: u64, delta_ms: f64) {
        if let Some(tick_fn) = self.vtable.tick {
            unsafe {
                tick_fn(self.user_data, tick_num, delta_ms);
            }
        }
    }

    /// Run the plugin's shutdown hook.
    pub fn shutdown(&self, reason: &str) {
        let reason_c = std::ffi::CString::new(reason).unwrap_or_default();
        if let Some(shutdown_fn) = self.vtable.shutdown {
            unsafe {
                shutdown_fn(self.user_data, reason_c.as_ptr());
            }
        }
    }

    /// Deliver an event to the plugin.
    pub fn handle_event(&self, event_id: u32, data: &[u8]) -> EventCallbackResult {
        let Some(handle_fn) = self.vtable.handle_event else {
            return EventCallbackResult::Continue;
        };
        unsafe { handle_fn(event_id, data.as_ptr(), data.len(), self.user_data) }
    }

    /// Log a message through the plugin's logging facility (if available).
    pub fn log(&self, level: LogLevel, message: &str) {
        let Some(log_fn) = self.vtable.log else {
            return;
        };
        let msg_c = std::ffi::CString::new(message).unwrap_or_default();
        unsafe {
            log_fn(level, msg_c.as_ptr(), self.user_data);
        }
    }

    /// Return the plugin metadata (borrowed).
    #[must_use]
    pub fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    /// Return the number of commands registered by this plugin.
    #[must_use]
    pub fn command_count(&self) -> u32 {
        self.vtable
            .get_command_count
            .map_or(0, |f| unsafe { f() })
    }
}

impl Drop for NativePluginHandle {
    fn drop(&mut self) {
        debug!(
            "Dropping native plugin handle for '{}'",
            self.metadata.name,
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_packing_roundtrip() {
        let v = PluginApiVersion {
            major: 2,
            minor: 1,
            patch: 3,
        };
        let packed = v.packed();
        assert_eq!(packed >> 22, 2);
        assert_eq!((packed >> 12) & 0x3FF, 1);
        assert_eq!(packed & 0xFFF, 3);
    }

    #[test]
    fn version_compatibility() {
        let server = PluginApiVersion {
            major: 2,
            minor: 0,
            patch: 0,
        };
        let plugin_ok = PluginApiVersion {
            major: 2,
            minor: 5,
            patch: 99,
        };
        let plugin_bad = PluginApiVersion {
            major: 3,
            minor: 0,
            patch: 0,
        };

        assert!(server.compatible_with(&plugin_ok));
        assert!(!server.compatible_with(&plugin_bad));
    }
}
