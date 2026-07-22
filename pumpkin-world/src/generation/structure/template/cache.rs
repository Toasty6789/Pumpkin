//! Template caching for embedded structure templates.
//!
//! This module provides a lazy-loading cache for structure templates that are
//! embedded in the binary at compile time using `include_bytes!`.

use std::sync::Arc;

use dashmap::DashMap;

use super::{StructureTemplate, structure_template::TemplateError};

/// A cache for loaded structure templates.
///
/// Templates are loaded lazily on first access and stored for reuse.
/// The cache is thread-safe and can be accessed from multiple threads.
pub struct TemplateCache {
    cache: DashMap<String, Arc<StructureTemplate>>,
}

impl Default for TemplateCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateCache {
    /// Creates a new empty template cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    /// Gets a template by `name`, loading it from embedded resources if not cached.
    ///
    /// Returns the loaded template wrapped in an `Arc`, or `None` if the template
    /// doesn't exist or failed to load.
    pub fn get(&self, name: &str) -> Option<Arc<StructureTemplate>> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);

        // Check cache first
        if let Some(template) = self.cache.get(name) {
            return Some(Arc::clone(&template));
        }

        // Try to load the template
        let bytes = Self::load_template_bytes(name)?;

        match StructureTemplate::from_nbt_bytes(bytes) {
            Ok(template) => {
                let arc = Arc::new(template);
                self.cache.insert(name.to_owned(), Arc::clone(&arc));
                Some(arc)
            }
            Err(e) => {
                tracing::error!("Failed to load template '{}': {}", name, e);
                None
            }
        }
    }

    /// Gets a template by name, returning an error if loading fails.
    ///
    /// # Errors
    ///
    /// Returns an error if the template doesn't exist or fails to parse.
    pub fn get_or_error(&self, name: &str) -> Result<Arc<StructureTemplate>, TemplateError> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);

        // Check cache first
        if let Some(template) = self.cache.get(name) {
            return Ok(Arc::clone(&template));
        }

        // Try to load the template
        let bytes = Self::load_template_bytes(name)
            .ok_or(TemplateError::MissingField("template file not found"))?;

        let template = StructureTemplate::from_nbt_bytes(bytes)?;
        let arc = Arc::new(template);
        self.cache.insert(name.to_owned(), Arc::clone(&arc));
        Ok(arc)
    }

    /// Preloads a list of templates into the cache.
    ///
    /// This can be useful during server startup to avoid loading delays
    /// during gameplay.
    pub fn preload(&self, names: &[&'static str]) {
        for name in names {
            if let Err(e) = self.get_or_error(name) {
                tracing::warn!("Failed to preload template '{}': {}", name, e);
            }
        }
    }

    /// Returns the number of cached templates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Returns whether the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Clears all cached templates.
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Loads raw template bytes from embedded resources.
    ///
    /// This function maps template names to their embedded byte data.
    /// Add new templates here as they are added to the assets.
    fn load_template_bytes(path: &str) -> Option<&'static [u8]> {
        get_template_bytes(path)
    }
}

include!(concat!(env!("OUT_DIR"), "/template_embeddings.rs"));

/// Global template cache instance.
///
/// This provides a singleton cache that can be used throughout the codebase
/// without needing to pass around a cache reference.
static GLOBAL_CACHE: std::sync::LazyLock<TemplateCache> =
    std::sync::LazyLock::new(TemplateCache::new);

/// Gets the global template cache.
#[must_use]
pub fn global_cache() -> &'static TemplateCache {
    &GLOBAL_CACHE
}

/// Gets a template by `name` from the global cache.
///
/// Returns the loaded template wrapped in an `Arc`, or `None` if not found.
#[must_use]
pub fn get_template(name: &str) -> Option<Arc<StructureTemplate>> {
    global_cache().get(name)
}

/// Preloads the templates for a group of structure pool IDs.
/// Called during server initialization to avoid JIT loading delays.
pub fn preload_structure_group(group: &[&str]) {
    for pool_id in group {
        if let Some(pool) = super::super::structures::jigsaw::TemplatePool::discover(pool_id) {
            for element in &pool.elements {
                element.for_each_template(|_, _, _| {});
            }
        }
    }
}

/// Preloads all major jigsaw structure templates.
/// Call during server startup for fast structure generation.
pub fn preload_all_structure_templates() {
    let groups: &[&[&str]] = &[
        // Ancient City
        &[
            "minecraft:ancient_city/city_center",
            "minecraft:ancient_city/structures",
            "minecraft:ancient_city/walls",
            "minecraft:ancient_city/city/entrance",
            "minecraft:ancient_city/city_center/walls",
            "minecraft:ancient_city/walls/no_corners",
            "minecraft:ancient_city/sculk",
        ],
        // Bastion Remnant
        &[
            "minecraft:bastion/starts",
            "minecraft:bastion/bridge/starting_pieces",
            "minecraft:bastion/bridge/bridge_pieces",
            "minecraft:bastion/bridge/connectors",
            "minecraft:bastion/bridge/legs",
            "minecraft:bastion/bridge/ramparts",
            "minecraft:bastion/bridge/walls",
            "minecraft:bastion/hoglin_stable/starting_pieces",
            "minecraft:bastion/hoglin_stable/large_stables",
            "minecraft:bastion/hoglin_stable/small_stables",
            "minecraft:bastion/treasure/bases",
            "minecraft:bastion/treasure/brains",
            "minecraft:bastion/treasure/connectors",
            "minecraft:bastion/treasure/corners",
            "minecraft:bastion/treasure/entrances",
            "minecraft:bastion/treasure/extensions",
            "minecraft:bastion/units/center_pieces",
            "minecraft:bastion/units/edges",
            "minecraft:bastion/units/fillers",
            "minecraft:bastion/units/pathways",
            "minecraft:bastion/units/stages",
            "minecraft:bastion/units/wall_units",
        ],
        // Pillager Outpost
        &[
            "minecraft:pillager_outpost/base_plates",
            "minecraft:pillager_outpost/towers",
            "minecraft:pillager_outpost/feature_plates",
            "minecraft:pillager_outpost/features",
        ],
        // Trail Ruins
        &[
            "minecraft:trail_ruins/tower/tower_top",
            "minecraft:trail_ruins/tower/additions",
            "minecraft:trail_ruins/buildings",
            "minecraft:trail_ruins/buildings/grouped",
            "minecraft:trail_ruins/roads",
            "minecraft:trail_ruins/decor",
        ],
        // Trial Chambers
        &[
            "minecraft:trial_chambers/corridor",
            "minecraft:trial_chambers/hallway",
            "minecraft:trial_chambers/intersection",
            "minecraft:trial_chambers/chamber",
            "minecraft:trial_chambers/chamber/addon",
            "minecraft:trial_chambers/chamber/assembly",
            "minecraft:trial_chambers/chamber/eruption",
            "minecraft:trial_chambers/chamber/pedestal",
            "minecraft:trial_chambers/chamber/slanted",
            "minecraft:trial_chambers/spawner/breeze",
            "minecraft:trial_chambers/spawner/melee",
            "minecraft:trial_chambers/spawner/ranged",
            "minecraft:trial_chambers/spawner/slow_ranged",
            "minecraft:trial_chambers/spawner/small_melee",
            "minecraft:trial_chambers/decor",
            "minecraft:trial_chambers/chests/supply",
            "minecraft:trial_chambers/dispensers/chamber",
            "minecraft:trial_chambers/reward/all",
        ],
        // Villages
        &[
            "minecraft:village/plains/town_centers",
            "minecraft:village/plains/houses",
            "minecraft:village/plains/streets",
            "minecraft:village/plains/terminators",
            "minecraft:village/plains/decor",
            "minecraft:village/plains/trees",
            "minecraft:village/plains/zombie/houses",
            "minecraft:village/plains/zombie/streets",
            "minecraft:village/plains/zombie/terminators",
            "minecraft:village/desert/town_centers",
            "minecraft:village/desert/houses",
            "minecraft:village/desert/streets",
            "minecraft:village/desert/terminators",
            "minecraft:village/desert/decor",
            "minecraft:village/desert/zombie/houses",
            "minecraft:village/savanna/town_centers",
            "minecraft:village/savanna/houses",
            "minecraft:village/savanna/streets",
            "minecraft:village/savanna/terminators",
            "minecraft:village/savanna/zombie/houses",
            "minecraft:village/snowy/town_centers",
            "minecraft:village/snowy/houses",
            "minecraft:village/snowy/streets",
            "minecraft:village/snowy/terminators",
            "minecraft:village/snowy/zombie/houses",
            "minecraft:village/taiga/town_centers",
            "minecraft:village/taiga/houses",
            "minecraft:village/taiga/streets",
            "minecraft:village/taiga/terminators",
            "minecraft:village/taiga/zombie/houses",
        ],
    ];

    for group in groups {
        preload_structure_group(group);
    }
}
