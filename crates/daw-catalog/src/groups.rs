//! Kind factories laid out in groups: the shared core of the instrument and effect registries.

use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use daw_contracts::{ContractError, ErrorCode};

/// Group of built-in kinds: `daw-bootstrap` fills it when assembling the session (`register`).
const BUILTIN_GROUP: &str = "builtin";

/// A factory with a kind name. Needed because `kind()` is declared separately in each factory contract.
pub(crate) trait Kinded {
    /// The failure for a kind that is not registered (unknown instrument or unknown effect).
    const UNKNOWN: ErrorCode;

    fn kind(&self) -> &'static str;
}

struct Group<F: ?Sized> {
    name: String,
    factories: Vec<Arc<F>>,
}

/// Factories live in groups: built-in ones are registered once, while the plugin group (VST3) can be replaced on the fly
/// ("Scan plugins"). The lock is safe: the catalog is read by the command thread and export, not the audio thread
/// (which only holds instruments and effects already created).
pub(crate) struct FactoryGroups<F: ?Sized> {
    groups: RwLock<Vec<Group<F>>>,
}

impl<F: ?Sized> Default for FactoryGroups<F> {
    fn default() -> Self {
        Self { groups: RwLock::new(Vec::new()) }
    }
}

impl<F: Kinded + ?Sized> FactoryGroups<F> {
    /// Adds a built-in kind, replacing one with the same name.
    pub(crate) fn register(&self, factory: Box<F>) {
        let mut groups = self.write();
        let group = entry(&mut groups, BUILTIN_GROUP);
        group.factories.retain(|f| f.kind() != factory.kind());
        group.factories.push(Arc::from(factory));
    }

    pub(crate) fn replace_group(&self, name: &str, factories: Vec<Box<F>>) {
        let mut groups = self.write();
        entry(&mut groups, name).factories = factories.into_iter().map(Arc::from).collect();
    }

    /// The factory is returned as a cloned reference so the lock is released before `create`, which can be slow for a
    /// plugin. Unknown kind: `F::UNKNOWN` (`TR-002` or `GR-002`).
    pub(crate) fn find(&self, kind: &str) -> Result<Arc<F>, ContractError> {
        self.read()
            .iter()
            .flat_map(|group| &group.factories)
            .find(|f| f.kind() == kind)
            .cloned()
            .ok_or_else(|| F::UNKNOWN.with("kind", kind))
    }

    /// Factory descriptions in group order; a factory for which `describe` returns `None` is left out.
    pub(crate) fn describe_all<T>(&self, describe: impl Fn(&F) -> Option<T>) -> Vec<T> {
        self.read().iter().flat_map(|group| &group.factories).filter_map(|f| describe(f)).collect()
    }

    fn read(&self) -> RwLockReadGuard<'_, Vec<Group<F>>> {
        self.groups.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, Vec<Group<F>>> {
        self.groups.write().unwrap_or_else(PoisonError::into_inner)
    }
}

fn entry<'a, F: ?Sized>(groups: &'a mut Vec<Group<F>>, name: &str) -> &'a mut Group<F> {
    let index = groups.iter().position(|g| g.name == name).unwrap_or_else(|| {
        groups.push(Group { name: name.to_string(), factories: Vec::new() });
        groups.len() - 1
    });
    &mut groups[index]
}
