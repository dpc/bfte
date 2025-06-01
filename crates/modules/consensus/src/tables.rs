use bfte_consensus_core::module::ModuleId;
use bfte_util_db::def_table;

def_table! {
    modules_configs: ModuleId => crate::ModuleConfig
}
