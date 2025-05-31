use bfte_consensus_core::block::BlockRound;
use bfte_util_db::def_table;

def_table! {
    app_cur_round: () => BlockRound
}
