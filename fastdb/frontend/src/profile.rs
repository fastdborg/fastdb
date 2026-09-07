//! Primary statement counters and separately attributed forward-fetch target work.
use crate::QueryResult;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct QueryMetrics {
    pub rows_read: u64,
    pub rows_written: u64,
    pub fullscan_steps: u64,
    pub index_steps: u64,
    pub vm_steps: u64,
    pub sort_operations: u64,
    pub btree_seeks: u64,
    pub fetch_batches: u64,
    pub fetch_rows_read: u64,
    pub fetch_vm_steps: u64,
}
impl QueryMetrics {
    pub(crate) fn from_statement(statement: &turso_core::Statement) -> Self {
        let metrics = statement.metrics();
        Self {
            rows_read: metrics.rows_read,
            rows_written: metrics.rows_written,
            fullscan_steps: metrics.fullscan_steps,
            index_steps: metrics.index_steps,
            vm_steps: metrics.insn_executed,
            sort_operations: metrics.sort_operations,
            btree_seeks: metrics.btree_seeks,
            fetch_batches: 0,
            fetch_rows_read: 0,
            fetch_vm_steps: 0,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct ProfiledQuery {
    pub result: QueryResult,
    pub metrics: QueryMetrics,
}
