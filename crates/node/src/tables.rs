use bfte_util_db::def_table;

def_table! {
    /// Tracks consensus database/schema version
    ui_pass_hash: () => [u8; 32]
}
