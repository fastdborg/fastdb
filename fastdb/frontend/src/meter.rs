//! Retained work for checked customer statements.
use crate::{Connection, Error, Parameters, QueryResult, Result, ResultLimits};
use serde::Serialize;
use std::sync::Arc;
use turso_core::execution_meter::{ExecutionLimits, ExecutionMeter};

#[derive(Debug, Clone, Copy, Default)]
pub struct ReadWorkLimits {
    pub max_rows_read: Option<u64>,
    pub max_vm_steps: Option<u64>,
}

/// Instrumented customer-statement work, including failed execution. Physical
/// engine counter coverage still applies; this is not a logical-write counter.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ReadWork {
    pub rows_read: u64,
    pub vm_steps: u64,
    pub read_budget_exhausted: bool,
    pub vm_budget_exhausted: bool,
}

#[derive(Debug)]
pub struct MeteredRead {
    pub outcome: Result<QueryResult>,
    pub work: ReadWork,
}

/// Limits shared by customer reads and row mutations within one checked write.
#[derive(Debug, Clone, Copy, Default)]
pub struct WriteWorkLimits {
    pub max_rows_read: Option<u64>,
    pub max_row_mutations: Option<u64>,
    pub max_vm_steps: Option<u64>,
}

/// Attempted work survives rollback. `row_mutations` is never a committed-write
/// receipt: the caller must resolve the enclosing transaction separately.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct WriteWork {
    pub rows_read: u64,
    pub row_mutations: u64,
    pub vm_steps: u64,
    pub read_budget_exhausted: bool,
    pub mutation_budget_exhausted: bool,
    pub vm_budget_exhausted: bool,
}

#[derive(Debug)]
pub struct MeteredWrite {
    pub outcome: Result<QueryResult>,
    pub work: WriteWork,
}

struct Scope<'a>(&'a Connection);
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        // Statement locals unwind before this scope, including after a panic.
        let _ = self.0.engine.set_execution_meter(None);
        *self.0.work_meter.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

impl Connection {
    /// Execute a checked SELECT with retained work on success and failure.
    /// Includes primary SQL/lowered statements and linked target statements;
    /// frontend catalog lookups and transaction cleanup are excluded. Uses the
    /// same accepted SELECT syntax as `profile_select_with_limits`, plus a bare
    /// record SELECT.
    ///
    /// Serialize all operations on this connection. A read limit permits at most
    /// one crossing instrumented visit, which is retained. Parsing, planning,
    /// frontend evaluation and arbitrary extension work are not bounded by VM
    /// steps. This API does not certify complete billing coverage of every index
    /// method or temporary/spilled execution path.
    pub fn select_metered(
        &self,
        sql: &str,
        params: &Parameters,
        result_limits: ResultLimits,
        work_limits: ReadWorkLimits,
    ) -> MeteredRead {
        let meter = Arc::new(ExecutionMeter::with_limits(ExecutionLimits {
            max_rows_read: work_limits.max_rows_read,
            max_row_mutations: None,
            max_vm_steps: work_limits.max_vm_steps,
        }));
        let outcome = crate::parser_stack(|| {
            {
                let mut slot = self.work_meter.lock().unwrap_or_else(|e| e.into_inner());
                if slot.is_some() {
                    return Err(Error::Unsupported("nested metered execution".into()));
                }
                *slot = Some(meter.clone());
            }
            let _scope = Scope(self);
            match fastql_parser::parse(sql)? {
                fastql_parser::Statement::SelectRecord(target) => {
                    self.metered_record(&target, params, result_limits)
                }
                _ => self
                    .profile_select_with_limits(sql, params, result_limits)
                    .map(|profile| profile.result),
            }
        });
        let snapshot = meter.snapshot();
        MeteredRead {
            outcome,
            work: ReadWork {
                rows_read: snapshot.rows_read,
                vm_steps: snapshot.vm_steps,
                read_budget_exhausted: snapshot.read_budget_exhausted,
                vm_budget_exhausted: snapshot.vm_budget_exhausted,
            },
        }
    }

    /// Execute a checked data write with retained work and atomic result limits.
    /// Shares one meter across source reads and primary document mutations;
    /// catalog and rollback are excluded. Managed index SQL contributes read/VM
    /// work but does not add logical mutations.
    /// Retained mutations include rolled-back attempts. Successful execution in
    /// a caller transaction is still pending that transaction's final outcome.
    /// Virtual/search/materialized coverage
    /// require further qualification before this can serve as billing evidence.
    pub fn write_metered(
        &self,
        sql: &str,
        params: &Parameters,
        result_limits: ResultLimits,
        work_limits: WriteWorkLimits,
    ) -> MeteredWrite {
        let meter = Arc::new(ExecutionMeter::with_limits(ExecutionLimits {
            max_rows_read: work_limits.max_rows_read,
            max_row_mutations: work_limits.max_row_mutations,
            max_vm_steps: work_limits.max_vm_steps,
        }));
        let outcome = crate::parser_stack(|| {
            {
                let mut slot = self.work_meter.lock().unwrap_or_else(|e| e.into_inner());
                if slot.is_some() {
                    return Err(Error::Unsupported("nested metered execution".into()));
                }
                *slot = Some(meter.clone());
            }
            let _scope = Scope(self);
            self.write_with_result_limits(sql, params, result_limits)
        });
        let snapshot = meter.snapshot();
        MeteredWrite {
            outcome,
            work: WriteWork {
                rows_read: snapshot.rows_read,
                row_mutations: snapshot.row_mutations,
                vm_steps: snapshot.vm_steps,
                read_budget_exhausted: snapshot.read_budget_exhausted,
                mutation_budget_exhausted: snapshot.mutation_budget_exhausted,
                vm_budget_exhausted: snapshot.vm_budget_exhausted,
            },
        }
    }

    pub(crate) fn run_customer(
        &self,
        sql: &str,
        params: &[crate::EngineValue],
    ) -> Result<Vec<Vec<crate::EngineValue>>> {
        self.run_attributed(sql, params, false)
    }

    pub(crate) fn run_index_maintenance(
        &self,
        sql: &str,
        params: &[crate::EngineValue],
    ) -> Result<Vec<Vec<crate::EngineValue>>> {
        self.run_attributed(sql, params, true)
    }

    fn run_attributed(
        &self,
        sql: &str,
        params: &[crate::EngineValue],
        maintenance: bool,
    ) -> Result<Vec<Vec<crate::EngineValue>>> {
        let mut statement = self.prepare(sql)?;
        for (i, value) in params.iter().enumerate() {
            statement.bind_at(std::num::NonZeroUsize::new(i + 1).unwrap(), value.clone())?;
        }
        self.meter_statement_attributed(&mut statement, maintenance, crate::collect_rows)
    }

    fn metered_record(
        &self,
        record: &crate::Record,
        params: &Parameters,
        limits: ResultLimits,
    ) -> Result<QueryResult> {
        if let Some(name) = params.keys().next() {
            return Err(Error::Parameter(format!("unused binding {name}")));
        }
        self.atomic(|| {
            let collection = self.catalog(&record.table)?;
            let id = crate::normalized_id(record, &collection.name)?;
            let mut result = QueryResult::documents(Vec::new(), 0);
            let mut budget = crate::budget::ResultBudget::new(Some(limits), &result.columns)?;
            let mut statement = self.prepare(format!(
                "SELECT doc FROM {} WHERE id = ?1 LIMIT 1",
                crate::quote(&collection.storage)
            ))?;
            statement.bind_at(
                std::num::NonZeroUsize::new(1).unwrap(),
                crate::EngineValue::Blob(crate::Value::Record(id).encode()?),
            )?;
            let rows = self.meter_statement(&mut statement, crate::collect_rows)?;
            for row in rows {
                let output = vec![crate::Value::Object(crate::decode_document(&row[0])?)];
                budget.row(&output)?;
                result.rows.push(output);
            }
            Ok(result)
        })
    }

    pub(crate) fn meter_statement<T>(
        &self,
        statement: &mut turso_core::Statement,
        execute: impl FnOnce(&mut turso_core::Statement) -> Result<T>,
    ) -> Result<T> {
        self.meter_statement_attributed(statement, false, execute)
    }

    fn meter_statement_attributed<T>(
        &self,
        statement: &mut turso_core::Statement,
        maintenance: bool,
        execute: impl FnOnce(&mut turso_core::Statement) -> Result<T>,
    ) -> Result<T> {
        let meter = self
            .work_meter
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let Some(meter) = meter else {
            return execute(statement);
        };
        let meter = if maintenance {
            Arc::new(meter.without_row_mutations())
        } else {
            meter
        };
        self.engine.set_execution_meter(Some(meter))?;
        let result = execute(statement);
        // A callback error can leave a live Row. Reset before detaching, so no
        // active statement escapes the scope and cleanup cannot inherit its budget.
        let reset = if result.is_err() {
            statement.reset()
        } else {
            Ok(())
        };
        let detach = self.engine.set_execution_meter(None);
        if let Err(cleanup) = reset.and(detach) {
            return Err(Error::Rollback {
                cause: result
                    .as_ref()
                    .err()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "metered statement completed".into()),
                rollback: cleanup.to_string(),
            });
        }
        result
    }
}
