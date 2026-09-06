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
        let (conn, _) = self
            .inner
            .as_ref()
            .ok_or_else(|| error("database is closed"))?;
        let before = conn.transaction_state();
        let result = (|| -> fastdb::Result<serde_json::Value> {
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
        })();
        let result = match result {
            Ok(value) => serde_json::json!({"result":value}),
            Err(e) => serde_json::json!({"error":{"code":e.code(),"message":e.to_string()}}),
        };
        Ok(serde_json::json!({"version":1,"execution":result,"transaction":{"before":before,"after":conn.transaction_state()}}).to_string())
    }
}
