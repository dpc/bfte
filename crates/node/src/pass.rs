use std::sync::atomic::Ordering;

use bfte_db::Database;
use tracing::info;

use crate::{LOG_TARGET, Node, tables};

impl Node {
    pub(crate) async fn load_ui_pass_hash(db: &Database) -> Option<blake3::Hash> {
        let bytes = db
            .read_with_expect(|ctx| {
                let tbl = ctx.open_table(&tables::ui_pass_hash::TABLE)?;
                Ok(tbl.get(&())?.map(|g| g.value()))
            })
            .await;

        bytes.map(blake3::Hash::from_bytes)
    }

    pub(crate) async fn change_ui_pass(&self, pass: &str) {
        let pass_hash = blake3::hash(pass.as_bytes());
        self.db()
            .write_with_expect(|ctx| {
                let mut tbl = ctx.open_table(&tables::ui_pass_hash::TABLE)?;
                tbl.insert(&(), pass_hash.as_bytes())?;
                Ok(())
            })
            .await;

        *self.ui_pass_hash().lock().expect("Locking failed") = pass_hash;
        self.ui_pass_is_temporary().store(false, Ordering::SeqCst);

        info!(
            target: LOG_TARGET,
            "UI password changed"
        );
    }
}
