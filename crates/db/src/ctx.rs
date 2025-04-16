use std::sync::Arc;
use std::{ops, result};

use redb_bincode::{WriteTransaction, redb};

pub struct WriteTransactionCtx {
    commit_hook_order_lock: Arc<std::sync::Mutex<()>>,
    dbtx: WriteTransaction,
    on_commit: std::sync::Mutex<Vec<Box<dyn FnOnce() + 'static>>>,
}

impl WriteTransactionCtx {
    pub fn new(dbtx: WriteTransaction, commit_hook_order_lock: Arc<std::sync::Mutex<()>>) -> Self {
        Self {
            dbtx,
            on_commit: std::sync::Mutex::new(vec![]),
            commit_hook_order_lock,
        }
    }
}
impl ops::Deref for WriteTransactionCtx {
    type Target = WriteTransaction;

    fn deref(&self) -> &Self::Target {
        &self.dbtx
    }
}

impl ops::DerefMut for WriteTransactionCtx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.dbtx
    }
}

impl WriteTransactionCtx {
    pub fn on_commit(&self, f: impl FnOnce() + 'static) {
        self.on_commit
            .lock()
            .expect("Locking failed")
            .push(Box::new(f));
    }

    pub(super) fn commit(self) -> result::Result<(), redb::CommitError> {
        let Self {
            dbtx,
            on_commit,
            commit_hook_order_lock: commit_order_lock,
        } = self;

        // We're guaranteed there's only one write tx at the time,
        // but after the `commit` below, there is no longer any guarantees,
        // so theoretically hooks from different txes could run in a different
        // order than txes itself, which could be a source of very ellusive issues.
        let _guard = commit_order_lock.lock().expect("Can't fail");

        dbtx.commit()?;

        for hook in on_commit.lock().expect("Locking failed").drain(..) {
            hook();
        }
        Ok(())
    }
}
