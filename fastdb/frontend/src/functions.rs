//! Statically linked document accessors. No database callbacks or external I/O.
use crate::{index_scalar, path_value, Connection, Error, Result, Value};
use turso_ext::{scalar, ResultCode, Value as ExtValue, ValueType};

pub(crate) fn register(connection: &Connection) -> Result<()> {
    // The engine connection has not escaped Database::connect yet. Registration
    // is exclusive, and the temporary extension context is freed on every path.
    unsafe {
        let api = connection.engine._build_turso_ext();
        let result = [
            (
                c"__fastdb_scalar",
                document_scalar as turso_ext::ScalarFunction,
            ),
            (
                c"__fastdb_value",
                document_value as turso_ext::ScalarFunction,
            ),
            (c"__fastdb_sort", document_sort as turso_ext::ScalarFunction),
        ]
        .into_iter()
        .try_for_each(|(name, callback)| {
            let code = (api.register_scalar_function)(
                api.ctx,
                name.as_ptr(),
                2,
                true,
                0,
                callback,
                None,
                None,
            );
            if code == ResultCode::OK {
                Ok(())
            } else {
                Err(Error::Storage(format!(
                    "accessor registration failed: {code:?}"
                )))
            }
        });
        connection.engine._free_extension_ctx(api);
        result
    }
}
fn get(args: &[ExtValue], mode: u8) -> Result<ExtValue> {
    if args.len() != 2 {
        return Err(Error::Validation("document accessor arity".into()));
    }
    // Outer joins produce a NULL document for the unmatched side.
    if args[0].value_type() == ValueType::Null {
        return Ok(ExtValue::null());
    }
    let bytes = args[0]
        .to_blob()
        .ok_or_else(|| Error::Storage("expected stored document".into()))?;
    let path: Vec<String> = serde_json::from_str(
        args[1]
            .to_text()
            .ok_or_else(|| Error::Storage("expected field path".into()))?,
    )?;
    let Value::Object(doc) = Value::decode(&bytes)? else {
        return Err(Error::Storage("expected object".into()));
    };
    let value = if path.is_empty() {
        Value::Object(doc)
    } else {
        read_path(&doc, &path)?.cloned().unwrap_or(Value::Null)
    };
    if mode == 1 {
        return Ok(ExtValue::from_blob(value.encode()?));
    }
    if mode == 2 {
        if let Value::Record(record) = &value {
            let mut key = record.table.to_ascii_lowercase().into_bytes();
            key.push(0);
            match &record.key {
                crate::Key::Integer(i) => {
                    key.push(0);
                    key.extend(((*i as u64) ^ (1u64 << 63)).to_be_bytes());
                }
                crate::Key::String(s) => {
                    key.push(1);
                    key.extend(s.as_bytes());
                }
            }
            return Ok(ExtValue::from_blob(key));
        }
    }
    let value = index_scalar(&value)?;
    use turso_core::{Numeric, Value as EngineValue};
    Ok(match value {
        EngineValue::Null => ExtValue::null(),
        EngineValue::Numeric(Numeric::Integer(i)) => ExtValue::from_integer(i),
        EngineValue::Numeric(Numeric::Float(n)) => ExtValue::from_float(n.into()),
        EngineValue::Text(t) => ExtValue::from_text(t.as_str().into()),
        EngineValue::Blob(b) => ExtValue::from_blob(b),
    })
}
fn read_path<'a>(doc: &'a crate::Document, path: &[String]) -> Result<Option<&'a Value>> {
    // Read traversal of a scalar parent is missing, unlike a SET write which
    // must reject replacing that parent implicitly.
    match path_value(doc, path) {
        Err(Error::Validation(_)) => Ok(None),
        result => result,
    }
}
#[scalar(name = "__fastdb_scalar")]
fn document_scalar(args: &[ExtValue]) -> ExtValue {
    get(args, 0).unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}
#[scalar(name = "__fastdb_value")]
fn document_value(args: &[ExtValue]) -> ExtValue {
    get(args, 1).unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}

#[scalar(name = "__fastdb_sort")]
fn document_sort(args: &[ExtValue]) -> ExtValue {
    get(args, 2).unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}
