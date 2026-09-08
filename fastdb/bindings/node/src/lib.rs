use napi_derive::napi;
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};
static NEXT_INTERRUPT: AtomicU64 = AtomicU64::new(1);
static INTERRUPTS: OnceLock<Mutex<BTreeMap<u64, fastdb::InterruptHandle>>> = OnceLock::new();
fn interrupts() -> &'static Mutex<BTreeMap<u64, fastdb::InterruptHandle>> {
    INTERRUPTS.get_or_init(|| Mutex::new(BTreeMap::new()))
}
#[napi]
pub fn interrupt_connection(key: String) -> bool {
    let Ok(key) = key.parse::<u64>() else {
        return false;
    };
    let handle = interrupts()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned();
    handle.is_some_and(|h| h.interrupt())
}
static NEXT_CANCELLATION: AtomicU64 = AtomicU64::new(1);
static CANCELLATIONS: OnceLock<Mutex<BTreeMap<u64, fastdb::CancellationToken>>> = OnceLock::new();
fn cancellations() -> &'static Mutex<BTreeMap<u64, fastdb::CancellationToken>> {
    CANCELLATIONS.get_or_init(|| Mutex::new(BTreeMap::new()))
}
#[napi]
pub fn create_cancellation_token() -> napi::Result<String> {
    let mut tokens = cancellations().lock().unwrap_or_else(|e| e.into_inner());
    if tokens.len() >= 16384 {
        return Err(error("cancellation token limit exceeded"));
    }
    let id = NEXT_CANCELLATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| error("cancellation identifiers exhausted"))?;
    tokens.insert(id, fastdb::CancellationToken::new());
    Ok(id.to_string())
}
#[napi]
pub fn cancel_operation(key: String) -> bool {
    let Ok(key) = key.parse::<u64>() else {
        return false;
    };
    let token = cancellations()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned();
    if let Some(token) = token {
        token.cancel();
        true
    } else {
        false
    }
}
#[napi]
pub fn release_cancellation_token(key: String) {
    if let Ok(key) = key.parse::<u64>() {
        cancellations()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
    }
}
fn error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}
/// Standalone typed construction; no database connection is opened.
#[napi]
pub fn vector_from_components(
    encoding: String,
    components: napi::bindgen_prelude::Buffer,
) -> napi::Result<napi::bindgen_prelude::Buffer> {
    if components.is_empty() || components.len() % 8 != 0 || components.len() > 65_536 * 8 {
        return Err(error("vector components require 1..65536 binary64 values"));
    }
    let values = components
        .chunks_exact(8)
        .map(|bytes| f64::from_le_bytes(bytes.try_into().expect("binary64 width")))
        .collect::<Vec<_>>();
    let value = if encoding == "float64" {
        fastdb::Value::vector64(&values)
    } else {
        let values = values.iter().map(|value| *value as f32).collect::<Vec<_>>();
        match encoding.as_str() {
            "float32" => fastdb::Value::vector32(&values),
            "sparse32" => fastdb::Value::vector32_sparse(&values),
            "quantized8" => fastdb::Value::vector8(&values),
            "bit1" => fastdb::Value::vector1bit(&values),
            _ => return Err(error("unknown vector encoding")),
        }
    }
    .map_err(error)?;
    let fastdb::Value::Vector(bytes) = value else {
        unreachable!("typed vector constructor")
    };
    Ok(bytes.into())
}
#[napi]
pub fn vector_from_sparse_entries(
    dimensions: f64,
    entries: napi::bindgen_prelude::Buffer,
) -> napi::Result<napi::bindgen_prelude::Buffer> {
    if !dimensions.is_finite()
        || dimensions.fract() != 0.0
        || !(1.0..=65_536.0).contains(&dimensions)
    {
        return Err(error("vector dimensions must be 1..65536"));
    }
    let dimensions = dimensions as usize;
    if entries.len() % 12 != 0 || entries.len() / 12 > dimensions {
        return Err(error("sparse entries require bounded index/binary64 pairs"));
    }
    let entries = entries
        .chunks_exact(12)
        .map(|bytes| {
            let index = u32::from_le_bytes(bytes[..4].try_into().expect("index width"));
            let value = f64::from_le_bytes(bytes[4..].try_into().expect("binary64 width"));
            (index as usize, value as f32)
        })
        .collect::<Vec<_>>();
    let fastdb::Value::Vector(bytes) =
        fastdb::Value::vector32_sparse_entries(dimensions, &entries).map_err(error)?
    else {
        unreachable!("typed vector constructor")
    };
    Ok(bytes.into())
}
#[napi]
pub struct NativeDatabase {
    interrupt_key: u64,
    inner: Option<(fastdb::Connection, fastdb::Database)>,
}
#[napi]
impl NativeDatabase {
    #[napi(constructor)]
    pub fn new(path: String) -> napi::Result<Self> {
        let db = fastdb::Database::open(&path).map_err(error)?;
        let conn = db.connect().map_err(error)?;
        let interrupt_key = NEXT_INTERRUPT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| error("interrupt identifiers exhausted"))?;
        interrupts()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(interrupt_key, conn.interrupt_handle());
        Ok(Self {
            interrupt_key,
            inner: Some((conn, db)),
        })
    }
    #[napi]
    pub fn close(&mut self) {
        self.close_inner();
    }
    #[napi]
    pub fn interrupt_key(&self) -> String {
        self.interrupt_key.to_string()
    }
    #[napi]
    pub fn execute(&self, sql: String, parameters: String) -> napi::Result<String> {
        self.report(|conn| {
            let parameters = decode_parameters(&parameters)?;
            query_value(conn.execute(&sql, &parameters)?)
        })
    }
    #[napi]
    pub fn execute_cancellable(
        &self,
        sql: String,
        parameters: String,
        key: String,
    ) -> napi::Result<String> {
        let key = key.parse::<u64>().map_err(error)?;
        let token = cancellations()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned()
            .ok_or_else(|| error("unknown cancellation token"))?;
        self.report(|conn| {
            let parameters = decode_parameters(&parameters)?;
            query_value(conn.execute_cancellable(&sql, &parameters, &token)?)
        })
    }
    #[napi]
    pub fn profile_select(
        &self,
        sql: String,
        parameters: String,
        cancellation_key: Option<String>,
    ) -> napi::Result<String> {
        let token = cancellation_key
            .map(|key| {
                let key = key.parse::<u64>().map_err(error)?;
                cancellations()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| error("unknown cancellation token"))
            })
            .transpose()?;
        self.report(|conn| {
            let params = decode_parameters(&parameters)?;
            let profile = if let Some(token) = &token { conn.profile_select_cancellable(&sql, &params, token)? } else { conn.profile_select(&sql, &params)? };
            let m = profile.metrics;
            Ok(serde_json::json!({"result":query_value(profile.result)?, "metrics":{
                "rowsRead":m.rows_read.to_string(), "rowsWritten":m.rows_written.to_string(),
                "fullscanSteps":m.fullscan_steps.to_string(), "indexSteps":m.index_steps.to_string(),
                "vmSteps":m.vm_steps.to_string(), "sortOperations":m.sort_operations.to_string(),
                "btreeSeeks":m.btree_seeks.to_string(),
                "fetchBatches":m.fetch_batches.to_string(), "fetchRowsRead":m.fetch_rows_read.to_string(),
                "fetchVmSteps":m.fetch_vm_steps.to_string()
            }}))
        })
    }
    #[napi]
    pub fn profile_select_with_limits(
        &self,
        sql: String,
        parameters: String,
        max_rows: String,
        max_payload_bytes: String,
        cancellation_key: Option<String>,
    ) -> napi::Result<String> {
        let token = cancellation_key
            .map(|key| {
                let key = key.parse::<u64>().map_err(error)?;
                cancellations()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| error("unknown cancellation token"))
            })
            .transpose()?;
        self.report(|conn| {
            let params = decode_parameters(&parameters)?;
            let limits = fastdb::ResultLimits {
                max_rows: max_rows.parse().map_err(|_| fastdb::Error::Validation("invalid result row limit".into()))?,
                max_payload_bytes: max_payload_bytes.parse().map_err(|_| fastdb::Error::Validation("invalid result payload limit".into()))?,
            };
            let profile = if let Some(token) = &token { conn.profile_select_with_limits_cancellable(&sql, &params, limits, token)? } else { conn.profile_select_with_limits(&sql, &params, limits)? };
            let m = profile.metrics;
            Ok(serde_json::json!({"result":query_value(profile.result)?, "metrics":{
                "rowsRead":m.rows_read.to_string(), "rowsWritten":m.rows_written.to_string(),
                "fullscanSteps":m.fullscan_steps.to_string(), "indexSteps":m.index_steps.to_string(),
                "vmSteps":m.vm_steps.to_string(), "sortOperations":m.sort_operations.to_string(),
                "btreeSeeks":m.btree_seeks.to_string(),
                "fetchBatches":m.fetch_batches.to_string(), "fetchRowsRead":m.fetch_rows_read.to_string(),
                "fetchVmSteps":m.fetch_vm_steps.to_string()
            }}))
        })
    }
    #[napi]
    pub fn check_collection_integrity(
        &self,
        table: String,
        max_documents: String,
        max_encoded_bytes: String,
        cancellation_key: Option<String>,
    ) -> napi::Result<String> {
        let token = cancellation_key
            .map(|key| {
                let key = key.parse::<u64>().map_err(error)?;
                cancellations()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| error("unknown cancellation token"))
            })
            .transpose()?;
        self.report(|conn| {
            let mut limits=fastdb::IntegrityLimits::default();
            if !max_documents.is_empty() {
                limits.max_documents=max_documents.parse().map_err(|_|fastdb::Error::Validation("invalid integrity document limit".into()))?;
            }
            if !max_encoded_bytes.is_empty() {
                limits.max_encoded_bytes=max_encoded_bytes.parse().map_err(|_|fastdb::Error::Validation("invalid integrity byte limit".into()))?;
            }
            let report=if let Some(token) = &token { conn.check_collection_integrity_cancellable(&table,limits,token)? } else { conn.check_collection_integrity(&table,limits)? };
            Ok(serde_json::json!({"documents":report.documents.to_string(),"indexes":report.indexes.to_string(),"indexEntries":report.index_entries.to_string(),"encodedBytes":report.encoded_bytes.to_string()}))
        })
    }
    #[napi]
    pub fn execute_batch(
        &self,
        script: String,
        cancellation_key: Option<String>,
    ) -> napi::Result<String> {
        let token = cancellation_key
            .map(|key| {
                let key = key.parse::<u64>().map_err(error)?;
                cancellations()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| error("unknown cancellation token"))
            })
            .transpose()?;
        self.report(|conn| {
            let mut entries=Vec::new();
            let visitor = |entry: fastdb::BatchExecution| {
                let execution=entry.execution;
                let result=execution.result.and_then(query_value);
                let proceed=result.is_ok();
                let mut value=match result {
                    Ok(result)=>serde_json::json!({"result":result}),
                    Err(error)=>serde_json::json!({"error":{"code":error.code(),"message":error.to_string()}}),
                };
                value["offset"]=entry.offset.into();
                value["transaction"]=serde_json::json!({"before":execution.transaction_before,"after":execution.transaction_after});
                entries.push(value);
                Ok(proceed)
            };
            if let Some(token) = &token { conn.visit_batch_cancellable(&script, token, visitor)?; } else { conn.visit_batch(&script, visitor)?; }
            Ok(serde_json::Value::Array(entries))
        })
    }
    #[napi]
    pub fn export_documents(
        &self,
        table: String,
        format: String,
        cancellation_key: Option<String>,
    ) -> napi::Result<String> {
        let token = cancellation_key
            .map(|key| {
                let key = key.parse::<u64>().map_err(error)?;
                cancellations()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| error("unknown cancellation token"))
            })
            .transpose()?;
        self.report(|conn| {
            let format = transfer_format(&format)?;
            let output = if let Some(token) = &token {
                conn.export_documents_cancellable(&table, format, token)?
            } else {
                conn.export_documents(&table, format)?
            };
            Ok(serde_json::Value::String(output))
        })
    }
    #[napi]
    pub fn import_documents(
        &self,
        table: String,
        input: String,
        format: String,
        cancellation_key: Option<String>,
    ) -> napi::Result<String> {
        let token = cancellation_key
            .map(|key| {
                let key = key.parse::<u64>().map_err(error)?;
                cancellations()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| error("unknown cancellation token"))
            })
            .transpose()?;
        self.report(|conn| {
            let format = transfer_format(&format)?;
            let imported = if let Some(token) = &token {
                conn.import_documents_cancellable(&table, &input, format, token)?
            } else {
                conn.import_documents(&table, &input, format)?
            };
            Ok(serde_json::json!({"imported": imported}))
        })
    }
    #[napi]
    pub fn migrate(&self, input: String, cancellation_key: Option<String>) -> napi::Result<String> {
        let token = cancellation_key
            .map(|key| {
                let key = key.parse::<u64>().map_err(error)?;
                cancellations()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| error("unknown cancellation token"))
            })
            .transpose()?;
        self.report(|conn| {
            let plan: serde_json::Value=serde_json::from_str(&input)?;
            let invalid=||fastdb::Error::Validation("expected migration objects with version, name and sql strings".into());
            let plan=plan.as_array().ok_or_else(invalid)?.iter().map(|m| {
                let field=|name|m.get(name).and_then(|v|v.as_str()).ok_or_else(invalid);
                let version=field("version")?.parse::<i64>().map_err(|_|invalid())?;
                Ok(fastdb::Migration {version,name:field("name")?.into(),sql:field("sql")?.into()})
            }).collect::<fastdb::Result<Vec<_>>>()?;
            let report=if let Some(token) = &token { conn.migrate_cancellable(&plan, token)? } else { conn.migrate(&plan)? };
            Ok(serde_json::json!({"alreadyApplied":report.already_applied,"applied":report.applied.iter().map(i64::to_string).collect::<Vec<_>>()}))
        })
    }
}
fn transfer_format(format: &str) -> fastdb::Result<fastdb::TransferFormat> {
    match format {
        "json" => Ok(fastdb::TransferFormat::Json),
        "ndjson" => Ok(fastdb::TransferFormat::Ndjson),
        _ => Err(fastdb::Error::Validation(
            "expected json or ndjson transfer format".into(),
        )),
    }
}
impl Drop for NativeDatabase {
    fn drop(&mut self) {
        self.close_inner();
    }
}
impl NativeDatabase {
    fn close_inner(&mut self) {
        interrupts()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.interrupt_key);
        self.inner.take();
    }
    fn report(
        &self,
        operation: impl FnOnce(&fastdb::Connection) -> fastdb::Result<serde_json::Value>,
    ) -> napi::Result<String> {
        let (conn, _) = self
            .inner
            .as_ref()
            .ok_or_else(|| error("database is closed"))?;
        let before = conn.transaction_state();
        let result = operation(conn);
        let result = match result {
            Ok(value) => serde_json::json!({"result":value}),
            Err(e) => serde_json::json!({"error":{"code":e.code(),"message":e.to_string()}}),
        };
        Ok(serde_json::json!({"version":1,"execution":result,"transaction":{"before":before,"after":conn.transaction_state()}}).to_string())
    }
}

fn decode_parameters(input: &str) -> fastdb::Result<fastdb::Parameters> {
    let values: BTreeMap<String, serde_json::Value> = serde_json::from_str(input)?;
    values
        .into_iter()
        .map(|(key, value)| fastdb::Value::from_portable_value(value).map(|v| (key, v)))
        .collect()
}

fn query_value(result: fastdb::QueryResult) -> fastdb::Result<serde_json::Value> {
    let rows = result
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(fastdb::Value::to_portable_value)
                .collect::<fastdb::Result<Vec<_>>>()
        })
        .collect::<fastdb::Result<Vec<_>>>()?;
    Ok(
        serde_json::json!({"columns":result.columns,"rows":rows,"affected":result.affected.to_string()}),
    )
}
