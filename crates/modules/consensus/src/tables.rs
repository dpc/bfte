use bfte_consensus_core::module::ModuleId;
use bfte_util_db::def_table;

def_table! {
    module_setup: ModuleId => crate::ModuleConfig
}
