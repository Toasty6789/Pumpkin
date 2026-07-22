//! C-ABI native plugin API for Pumpkin.
//!
//! This module provides everything needed to write a **native** (compiled)
//! Pumpkin plugin that communicates with the server through a stable C-ABI
//! boundary, using `#[repr(C)]` types and `#[no_mangle]` extern symbols.
//!
//! Unlike the WASM plugin path (which uses `wit-bindgen` + `wasmtime`),
//! native plugins are dynamic libraries (`.dll` / `.so` / `.dylib`) loaded
//! at runtime with `libloading`.  This gives them:
//!
//! - Full access to the host's memory, threading, and I/O.
//! - No sandboxing overhead — performance-critical plugins (e.g. anti-cheat,
//!   world-gen) benefit from direct CPU access.
//! - A stable, version-negotiated ABI that guarantees backward compatibility
//!   within the same major version.
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use pumpkin_plugin_api::native::prelude::*;
//!
//! struct MyFirstPlugin;
//!
//! impl NativePlugin for MyFirstPlugin {
//!     fn metadata() -> PluginMetadata {
//!         PluginMetadata {
//!             name: c"Example Plugin".as_ptr(),
//!             version: c"1.0.0".as_ptr(),
//!             authors: c"you".as_ptr(),
//!             description: c"A minimal example.".as_ptr(),
//!             dependencies: c"".as_ptr(),
//!             permissions: c"".as_ptr(),
//!             api_version: PluginApiVersion::CURRENT,
//!         }
//!     }
//! }
//!
//! declare_native_plugin!(MyFirstPlugin);
//! ```
//!
//! # Version shielding
//!
//! The generated [`PLUGIN_API_VERSION`] symbol lets the server check
//! compatibility at load time.  A plugin compiled against major version `2`
//! will be rejected by a server running major version `3` (and vice versa),
//! preventing mysterious crashes.
//!
//! # Consistency guarantees
//!
//! Every public type in this module is `#[repr(C)]`.  Every exported function
//! follows the `extern "C"` ABI.  Strings are passed as `*const c_char` with
//! the plugin owning the backing memory for the call duration.
//!
//! [`PLUGIN_API_VERSION`]: types::PluginApiVersion

pub mod events;
pub mod types;
pub mod vtable;

/// Convenience re-exports for native plugin authors.
pub mod prelude {
    pub use super::{
        events::*,
        types::*,
        vtable::{NativePlugin, PluginVTable},
    };
    pub use crate::declare_native_plugin;
}
