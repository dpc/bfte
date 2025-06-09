// SPDX-License-Identifier: MIT

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Weak};
use std::{future, marker, ops};

use bfte_consensus_core::block::BlockRound;
use bfte_consensus_core::citem::{CItem, ModuleDyn};
use bfte_consensus_core::module::{ModuleId, ModuleKind};
use bfte_module::module::{DynModuleWithConfig, IModule};
use snafu::{OptionExt as _, Snafu};
use tokio::select;
use tokio::sync::{OwnedRwLockReadGuard, RwLock, RwLockReadGuard, RwLockWriteGuard, watch};
use tokio_stream::StreamMap;
use tokio_stream::wrappers::WatchStream;

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
pub struct SharedModules {
    update_tx: watch::Sender<()>,
    update_rx: watch::Receiver<()>,
    inner: Arc<RwLock<BTreeMap<ModuleId, DynModuleWithConfig>>>,
}

impl SharedModules {
    pub fn new() -> Self {
        let (update_tx, update_rx) = watch::channel(());
        Self {
            update_tx,
            update_rx,
            inner: Default::default(),
        }
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, BTreeMap<ModuleId, DynModuleWithConfig>> {
        self.inner.read().await
    }

    pub fn send_changed(&self) {
        self.update_tx.send_replace(());
    }

    pub async fn write(&self) -> RwLockWriteGuard<'_, BTreeMap<ModuleId, DynModuleWithConfig>> {
        self.inner.write().await
    }

    pub fn downgrade(&self) -> WeakSharedModules {
        WeakSharedModules {
            inner: Arc::downgrade(&self.inner),
            update_rx: self.update_rx.clone(),
        }
    }
}

impl Default for SharedModules {
    fn default() -> Self {
        Self::new()
    }
}

/// [`Self`] is a weak reference to [`SharedModules`]
#[derive(Clone)]
pub struct WeakSharedModules {
    update_rx: watch::Receiver<()>,
    inner: Weak<RwLock<BTreeMap<ModuleId, DynModuleWithConfig>>>,
}

impl WeakSharedModules {
    pub fn get_update_rx(&self) -> watch::Receiver<()> {
        self.update_rx.clone()
    }

    pub async fn wait_fresh_consensus_proposal(
        &self,
        finality_consensus_rx: watch::Receiver<BlockRound>,
        mut node_app_ack_rx: watch::Receiver<BlockRound>,
    ) -> Vec<CItem> {
        let mut shared_modules_changed_rx = self.get_update_rx();
        loop {
            // We only want to propose anything, if app layer is up to date with the
            // latest finality
            let Ok(_) = node_app_ack_rx
                .wait_for(|app_ack| *app_ack == *finality_consensus_rx.borrow())
                .await
            else {
                future::pending().await
            };

            shared_modules_changed_rx.mark_unchanged();

            select! {
                res = self.wait_consensus_proposal() => {
                    break res;
                }
                res = shared_modules_changed_rx.changed() => {

                    if res.is_err() {
                        future::pending().await
                    }
                    continue;
                }
            }
        }
    }

    /// Wait for any of the modules to return proposed citems
    ///
    /// This is supposed to get canceled from the outside,
    /// so just hangs if the modules underneath disappeared.
    async fn wait_consensus_proposal(&self) -> Vec<CItem> {
        let arc = self.upgrade_or_hang().await;

        let mut stream_map: StreamMap<ModuleId, _> = StreamMap::new();

        let read = arc.read().await;

        for (&module_id, module) in read.iter() {
            let citems_rx = module.propose_citems_rx().await;
            {
                let citems = citems_rx.borrow();
                if !citems.is_empty() {
                    return citems
                        .iter()
                        .map(|citem| CItem::PeerCItem(ModuleDyn::new(module_id, citem.clone())))
                        .collect();
                }
            }
            stream_map.insert(
                module_id,
                WatchStream::new(citems_rx).filter(|v| !v.is_empty()),
            );
        }

        // Important; We don't want to be holding the lock. Big part of why
        // `propose_citems_rx` returns watch channels - so we can wait on them
        // and detect modules being distroyed from undrneath as well.
        drop(read);

        use tokio_stream::StreamExt as _;
        if let Some((module_id, citems)) = stream_map.next().await {
            assert!(!citems.is_empty());
            citems
                .iter()
                .map(|citem| CItem::PeerCItem(ModuleDyn::new(module_id, citem.clone())))
                .collect()
        } else {
            future::pending().await
        }
    }

    async fn upgrade_or_hang(&self) -> Arc<RwLock<BTreeMap<ModuleId, DynModuleWithConfig>>> {
        let Some(arc) = self.inner.upgrade() else {
            std::future::pending().await
        };
        arc
    }

    /// Get an instance of one of the modules
    ///
    /// **WARNING**: Caller should not store the value as it might block
    /// `node-app` from acquiring the write lock on modules, preventing it
    /// from processing consensus modules reconfiguration.
    pub async fn get_module(
        &self,
        module_id: ModuleId,
    ) -> Option<OwnedRwLockReadGuard<BTreeMap<ModuleId, DynModuleWithConfig>, DynModuleWithConfig>>
    {
        let arc = self.upgrade_or_hang().await;

        let read = arc.read_owned().await;

        OwnedRwLockReadGuard::try_map(read, |tree| tree.get(&module_id)).ok()
    }

    pub async fn get_modules_ids(&self) -> BTreeSet<ModuleId> {
        let arc = self.upgrade_or_hang().await;

        let read = arc.read().await;

        read.keys().copied().collect()
    }
    pub async fn get_modules_kinds(&self) -> BTreeMap<ModuleId, ModuleKind> {
        let arc = self.upgrade_or_hang().await;

        let read = arc.read().await;

        read.iter()
            .map(|(id, module)| (*id, module.config.kind))
            .collect()
    }
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
    inner: OwnedRwLockReadGuard<BTreeMap<ModuleId, DynModuleWithConfig>, DynModuleWithConfig>,
    // This is here purely to prevent getters from storing it by mistake
    _marker: &'r marker::PhantomData<()>,
}

impl ops::Deref for SharedModuleRef<'_> {
    type Target = dyn IModule + Send + Sync;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}
