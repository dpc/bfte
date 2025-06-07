use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use bfte_db::Database;

use super::Node;
use crate::connection_pool::ConnectionPool;

impl Node {
    pub(crate) fn db(&self) -> &Arc<Database> {
        &self.db
    }

    pub(crate) fn connection_pool(&self) -> &ConnectionPool {
        &self.connection_pool
    }

    pub(crate) fn iroh_endpoint(&self) -> &iroh::Endpoint {
        &self.iroh_endpoint
    }

    pub(crate) fn ui_pass_hash(&self) -> &std::sync::Mutex<blake3::Hash> {
        &self.ui_pass_hash
    }

    pub(crate) fn ui_pass_is_temporary(&self) -> &AtomicBool {
        &self.ui_pass_is_temporary
    }

    pub(crate) fn peer_addr_needed(&self) -> &Arc<tokio::sync::Notify> {
        &self.peer_addr_needed
    }

    pub(crate) fn root_secret(&self) -> Option<bfte_derive_secret::DeriveableSecret> {
        self.root_secret
    }
}
