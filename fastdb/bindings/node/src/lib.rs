use napi_derive::napi;
use std::collections::BTreeMap;
fn error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}
#[napi]
pub struct NativeDatabase {
    inner: Option<(fastdb::Connection, fastdb::Database)>,
}
#[napi]
impl NativeDatabase {
    #[napi(constructor)]
    pub fn new(path: String) -> napi::Result<Self> {
        let db = fastdb::Database::open(&path).map_err(error)?;
        let conn = db.connect().map_err(error)?;
        Ok(Self {
            inner: Some((conn, db)),
        })
    }
    #[napi]
    pub fn close(&mut self) {
        self.inner.take();
    }
    #[napi]
    pub fn execute(&self, sql: String, parameters: String) -> napi::Result<String> {
        self.report(|conn| {
            let parameters: BTreeMap<String, serde_json::Value> =
                serde_json::from_str(&parameters)?;
            let parameters = parameters
                .into_iter()
                .map(|(key, value)| fastdb::Value::from_portable_value(value).map(|v| (key, v)))
                .collect::<fastdb::Result<fastdb::Parameters>>()?;
            let result = conn.execute(&sql, &parameters)?;
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
        })
    }
    #[napi]
    pub fn export_documents(&self, table: String, format: String) -> napi::Result<String> {
        self.report(|conn| {
            Ok(serde_json::Value::String(
                conn.export_documents(&table, transfer_format(&format)?)?,
            ))
        })
    }
    #[napi]
    pub fn import_documents(
        &self,
        table: String,
        input: String,
        format: String,
    ) -> napi::Result<String> {
        self.report(|conn| Ok(serde_json::json!({"imported":conn.import_documents(&table,&input,transfer_format(&format)?)?})))
    }
    #[napi]
    pub fn migrate(&self, input: String) -> napi::Result<String> {
        self.report(|conn| {
            let plan: serde_json::Value=serde_json::from_str(&input)?;
            let invalid=||fastdb::Error::Validation("expected migration objects with version, name and sql strings".into());
            let plan=plan.as_array().ok_or_else(invalid)?.iter().map(|m| {
                let field=|name|m.get(name).and_then(|v|v.as_str()).ok_or_else(invalid);
                let version=field("version")?.parse::<i64>().map_err(|_|invalid())?;
                Ok(fastdb::Migration {version,name:field("name")?.into(),sql:field("sql")?.into()})
            }).collect::<fastdb::Result<Vec<_>>>()?;
            let report=conn.migrate(&plan)?;
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
impl NativeDatabase {
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
