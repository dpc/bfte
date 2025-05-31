// SPDX-License-Identifier: MIT

use std::collections::BTreeMap;
use std::sync::{Arc, Weak};
use std::{marker, ops};

use bfte_consensus_core::module::ModuleId;
use bfte_module::module::{DynModule, IModule};
use snafu::{OptionExt as _, Snafu};
use tokio::sync::{OwnedRwLockReadGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Shared module state
///
/// Sharing module state between components is a bit tricky,
/// so this structure facilitates it.
///
/// Modules (instances of `DynModule`) are a resource, as they
/// might hold hold references to futures running in the background
/// of the modules etc.
///
/// `bfte-node-app` is a primary owner of modules, as it initializes them,
/// destroys and changes their composition.
///
/// `bfte-node` needs an access to modules, but "weaker" one, as it
/// only needs it for read-only purposes like requesting consensus items
/// proposals and maybe handling UI/API requests.
///
/// [`SharedModules`] is the strong, "owning", reference.
#[derive(Default)]
pub struct SharedModules {
    inner: Arc<RwLock<BTreeMap<ModuleId, DynModule>>>,
}

impl SharedModules {
    pub async fn read(&self) -> RwLockReadGuard<'_, BTreeMap<ModuleId, DynModule>> {
        self.inner.read().await
    }
    pub async fn write(&self) -> RwLockWriteGuard<'_, BTreeMap<ModuleId, DynModule>> {
        self.inner.write().await
    }

    pub fn downgrade(&self) -> WeakSharedModules {
        WeakSharedModules {
            inner: Arc::downgrade(&self.inner),
        }
    }
}
/// [`Self`] is a weak reference to [`SharedModules`]
pub struct WeakSharedModules {
    inner: Weak<RwLock<BTreeMap<ModuleId, DynModule>>>,
}

#[derive(Debug, Snafu)]
pub enum ModuleGetError {
    /// If the underlying modules are gone, it must mean that their owner
    /// (`bfte-node`) is gone, which probably means the process is shutting
    /// down for some reason.
    ShuttingDown,
}
type ModuleGetResult<T> = Result<T, ModuleGetError>;

impl WeakSharedModules {
    pub async fn get(&self, id: ModuleId) -> ModuleGetResult<Option<SharedModuleRef<'_>>> {
        let arc = self.inner.upgrade().context(ShuttingDownSnafu)?;

        let read = arc.read_owned().await;
        let Some(read) = OwnedRwLockReadGuard::<_>::try_map(read, |map| map.get(&id)).ok() else {
            return Ok(None);
        };

        Ok(Some(SharedModuleRef {
            inner: read,
            _marker: &marker::PhantomData,
        }))
    }
}

pub struct SharedModuleRef<'r> {
    inner: OwnedRwLockReadGuard<BTreeMap<ModuleId, DynModule>, DynModule>,
    // This is here purely to prevent getters from storing it by mistake
    _marker: &'r marker::PhantomData<()>,
}

impl ops::Deref for SharedModuleRef<'_> {
    type Target = dyn IModule + Send + Sync;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}
