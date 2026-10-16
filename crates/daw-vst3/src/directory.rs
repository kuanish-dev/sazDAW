//! `PluginDirectory`: "know which found plugin stands behind a catalog kind" (`vst3:distortion` -> plugin).
//!
//! Needed by plugin windows: an effect or instrument kind in the project is a string, but a window needs the plugin's bundle and class.
//! Sources (`PluginSource`) are shared with the catalog factories so a bundle loads once.

use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

use crate::source::PluginSource;

#[derive(Default)]
pub struct PluginDirectory {
    sources: RwLock<HashMap<String, Arc<PluginSource>>>,
}

impl PluginDirectory {
    /// Replaces the whole set (like a catalog group: plugins gone from disk disappear).
    pub fn replace(&self, sources: HashMap<String, Arc<PluginSource>>) {
        *self.sources.write().unwrap_or_else(PoisonError::into_inner) = sources;
    }

    /// Catalog kind and the plugin behind it: the installed plugin list and previous sources for republishing.
    pub fn entries(&self) -> Vec<(String, Arc<PluginSource>)> {
        self.sources.read().unwrap_or_else(PoisonError::into_inner).iter().map(|(kind, source)| (kind.clone(), source.clone())).collect()
    }

    pub fn find(&self, kind: &str) -> Option<Arc<PluginSource>> {
        self.sources.read().unwrap_or_else(PoisonError::into_inner).get(kind).cloned()
    }
}
