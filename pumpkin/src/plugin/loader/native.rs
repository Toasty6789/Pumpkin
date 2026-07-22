//! Native plugin loader — loads compiled dynamic-library plugins via
//! `libloading`, validates API version compatibility, and wraps them in
//! [`NativePluginHandle`] for safe interaction.
//!
//! ## C-ABI contract
//!
//! Every native plugin **must** expose the following `#[no_mangle]` symbols:
//!
//! | Symbol | Type | Required | Description |
//! |--------|------|----------|-------------|
//! | `PUMPKIN_API_VERSION` | `*const u32` | ✓ | Packed API version `(major<<22 \| minor<<12 \| patch)` |
//! | `PUMPKIN_PLUGIN_VTABLE` | `*const PluginVTable` | ✓ | Main vtable with all lifecycle hooks |
//!
//! The simplest way to produce these is via the
//! [`declare_native_plugin!`](pumpkin_plugin_api::declare_native_plugin!) macro.

use std::any::Any;
use std::path::Path;
use std::sync::Arc;

use libloading::Library;

use crate::plugin::api::native_api::NativePluginHandle;
use crate::plugin::loader::{LoaderError, PluginLoadFuture, PluginLoader, PluginUnloadFuture};
use crate::plugin::{Plugin, PluginMetadata};

/// Loader for native (compiled dynamic library) plugins.
pub struct NativePluginLoader;

impl PluginLoader for NativePluginLoader {
    fn load<'a>(&'a self, path: &'a Path) -> PluginLoadFuture<'a> {
        Box::pin(async move {
            let path = path.to_owned();

            // Open the dynamic library.
            let library = unsafe {
                Library::new(&path).map_err(|e| LoaderError::LibraryLoad(e.to_string()))?
            };
            let library = Arc::new(library);

            // Use NativePluginHandle to validate and load the vtable-based plugin.
            let handle = unsafe {
                NativePluginHandle::load(library.clone())
                    .map_err(LoaderError::InitializationFailed)?
            };

            // Build an adapter that turns the vtable plugin into the server's
            // internal Plugin trait so the rest of the plugin pipeline works
            // transparently.
            let plugin = NativePluginAdapter {
                metadata: handle.metadata.clone(),
                handle,
            };

            let metadata_for_loader = MetadataForLoader::from(&plugin.metadata);

            Ok((
                Box::new(plugin) as Box<dyn Plugin>,
                metadata_for_loader.into_server_metadata(),
                Box::new(library) as Box<dyn Any + Send + Sync>,
            ))
        })
    }

    fn can_load(&self, path: &Path) -> bool {
        let ext = path.extension().unwrap_or_default();

        if cfg!(target_os = "windows") {
            ext.eq_ignore_ascii_case("dll")
        } else if cfg!(target_os = "macos") {
            ext.eq_ignore_ascii_case("dylib")
        } else {
            ext.eq_ignore_ascii_case("so")
        }
    }

    fn unload(&self, data: Box<dyn Any + Send + Sync>) -> PluginUnloadFuture<'_> {
        Box::pin(async {
            let library = data
                .downcast::<Arc<Library>>()
                .map_err(|_| LoaderError::InvalidLoaderData)?;
            // Dropping the Arc will close the library when all references are gone.
            drop(library);
            Ok(())
        })
    }

    /// Windows locks loaded DLLs, so we cannot safely unload them.
    fn can_unload(&self) -> bool {
        !cfg!(target_os = "windows")
    }
}

// ---------------------------------------------------------------------------
// Adapter: wraps a NativePluginHandle as a server-side Plugin trait object
// ---------------------------------------------------------------------------

/// Adapts a [`NativePluginHandle`] to the server's internal [`Plugin`] trait.
///
/// This lets the existing plugin pipeline (event dispatch, command
/// registration, lifecycle) work with vtable-based native plugins without
/// any changes.
struct NativePluginAdapter {
    metadata: PluginMetadata,
    handle: NativePluginHandle,
}

// Safety: NativePluginHandle is Send + Sync.
unsafe impl Send for NativePluginAdapter {}
unsafe impl Sync for NativePluginAdapter {}

impl Plugin for NativePluginAdapter {
    fn on_load(
        &mut self,
        _server: Arc<crate::plugin::api::Context>,
    ) -> crate::plugin::api::PluginFuture<'_, Result<(), String>> {
        // The vtable's init was already called during NativePluginHandle::load.
        Box::pin(async move { Ok(()) })
    }

    fn on_unload(
        &mut self,
        _server: Arc<crate::plugin::api::Context>,
    ) -> crate::plugin::api::PluginFuture<'_, Result<(), String>> {
        self.handle.shutdown("plugin unloaded");
        Box::pin(async move { Ok(()) })
    }
}

// ---------------------------------------------------------------------------
// Temporary metadata representation for the loader pipeline
// ---------------------------------------------------------------------------

/// Minimal metadata struct matching the shape the loader pipeline expects.
struct MetadataForLoader {
    name: String,
    version: String,
    authors: Vec<String>,
    description: String,
    dependencies: Vec<String>,
    permissions: Vec<String>,
}

impl MetadataForLoader {
    fn from(m: &PluginMetadata) -> Self {
        Self {
            name: m.name.clone(),
            version: m.version.clone(),
            authors: m.authors.clone(),
            description: m.description.clone(),
            dependencies: m.dependencies.clone(),
            permissions: m.permissions.clone(),
        }
    }

    fn into_server_metadata(self) -> PluginMetadata {
        PluginMetadata {
            name: self.name,
            version: self.version,
            authors: self.authors,
            description: self.description,
            dependencies: self.dependencies,
            permissions: self.permissions,
        }
    }
}
