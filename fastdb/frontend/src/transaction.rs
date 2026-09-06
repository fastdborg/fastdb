//! Observable transaction state, sampled around a synchronous execution.
use crate::{Connection, Parameters, QueryResult, Result};
use serde::Serialize;

/// Whether the connection has an explicit transaction or an outer savepoint.
/// Autocommit does not distinguish a prior commit from a rollback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionState {
    Autocommit,
    Active,
}

/// Keeps the original typed result/error alongside transaction observations.
#[derive(Debug)]
pub struct ExecutionReport {
    pub result: Result<QueryResult>,
    pub transaction_before: TransactionState,
    pub transaction_after: TransactionState,
}
impl Connection {
    /// Read the pinned engine's current autocommit flag without issuing SQL.
    pub fn transaction_state(&self) -> TransactionState {
        if self.engine.get_auto_commit() {
            TransactionState::Autocommit
        } else {
            TransactionState::Active
        }
    }

    /// Execute and sample transaction state on both success and failure.
    /// Callers must serialize operations on this connection to attribute these
    /// observations to this execution. No commit/rollback cause is inferred.
    pub fn execute_report(&self, sql: &str, params: &Parameters) -> ExecutionReport {
        let transaction_before = self.transaction_state();
        let result = self.execute(sql, params);
        ExecutionReport {
            result,
            transaction_before,
            transaction_after: self.transaction_state(),
        }
    }
}

/// One executed script statement, located by UTF-8 byte offset.
#[derive(Debug)]
pub struct BatchExecution {
    pub offset: usize,
    pub execution: ExecutionReport,
}
impl Connection {
    /// Split the full script before executing, then stop on the first failure.
    /// There is no implicit batch transaction. Explicit transaction control
    /// belongs to the script; a failed batch may leave it active.
    pub fn execute_batch(&self, script: &str) -> Result<Vec<BatchExecution>> {
        let mut reports = Vec::new();
        self.visit_batch(script, |report| {
            reports.push(report);
            Ok(true)
        })?;
        Ok(reports)
    }

    /// Visit results between statements. False stops early; visitor errors are
    /// returned without rolling back earlier work. Execution errors are visited
    /// once and always stop the script. The full script is split before execution.
    pub fn visit_batch(
        &self,
        script: &str,
        mut visitor: impl FnMut(BatchExecution) -> Result<bool>,
    ) -> Result<()> {
        let statements = fastql_parser::split_script(script)?;
        for statement in statements {
            let execution = self.execute_report(statement.sql, &Parameters::new());
            let failed = execution.result.is_err();
            let proceed = visitor(BatchExecution {
                offset: statement.offset,
                execution,
            })?;
            if failed || !proceed {
                break;
            }
        }
        Ok(())
    }
}
