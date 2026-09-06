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
                c"__fastdb_vector_input",
                vector_input as turso_ext::ScalarFunction,
                1,
            ),
            (
                c"__fastdb_vector_value",
                vector_value as turso_ext::ScalarFunction,
                1,
            ),
            (c"__fastdb_pack", pack as turso_ext::ScalarFunction, 1),
            (
                c"__fastdb_nullable",
                nullable as turso_ext::ScalarFunction,
                1,
            ),
            (c"__fastdb_unwrap", unwrap as turso_ext::ScalarFunction, 1),
            (c"__fastdb_helper", helper as turso_ext::ScalarFunction, -1),
            (
                c"__fastdb_scalar",
                document_scalar as turso_ext::ScalarFunction,
                2,
            ),
            (
                c"__fastdb_value",
                document_value as turso_ext::ScalarFunction,
                2,
            ),
            (
                c"__fastdb_sort",
                document_sort as turso_ext::ScalarFunction,
                2,
            ),
            (
                c"__fastdb_record_value",
                record_value as turso_ext::ScalarFunction,
                2,
            ),
            (
                c"__fastdb_sort_encoded",
                sort_encoded as turso_ext::ScalarFunction,
                1,
            ),
        ]
        .into_iter()
        .try_for_each(|(name, callback, argc)| {
            let code = (api.register_scalar_function)(
                api.ctx,
                name.as_ptr(),
                argc,
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
        return ordered(&value);
    }
    scalar_result(&value)
}
fn ordered(value: &Value) -> Result<ExtValue> {
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
    scalar_result(value)
}
fn scalar_result(value: &Value) -> Result<ExtValue> {
    let value = index_scalar(value)?;
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

#[scalar(name = "__fastdb_record_value")]
fn record_value(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        if args.len() != 2 {
            return Err(Error::Validation(
                "record constructor expects target and key".into(),
            ));
        }
        let target = args[0]
            .to_text()
            .ok_or_else(|| Error::Validation("record target must be text".into()))?;
        let table = crate::canonical(target)?;
        let key = match args[1].value_type() {
            ValueType::Integer => crate::Key::Integer(args[1].to_integer().expect("integer value")),
            ValueType::Text => crate::Key::String(args[1].to_text().expect("text value").into()),
            _ => return Err(Error::Validation("record key must be text or int64".into())),
        };
        Ok(ExtValue::from_blob(
            Value::Record(crate::Record { table, key }).encode()?,
        ))
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}

#[scalar(name = "__fastdb_sort_encoded")]
fn sort_encoded(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        if args.len() != 1 {
            return Err(Error::Validation("sort arity".into()));
        }
        if args[0].value_type() == ValueType::Null {
            return Ok(ExtValue::null());
        }
        let bytes = args[0]
            .to_blob()
            .ok_or_else(|| Error::Validation("expected typed value".into()))?;
        ordered(&Value::decode(&bytes)?)
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}

fn decode_arg(arg: &ExtValue) -> Result<Value> {
    if arg.value_type() == ValueType::Null {
        return Ok(Value::Null);
    }
    Value::decode(
        &arg.to_blob()
            .ok_or_else(|| Error::Storage("expected encoded helper argument".into()))?,
    )
}
#[scalar(name = "__fastdb_pack")]
fn pack(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [v] = args else {
            return Err(Error::Validation("pack arity".into()));
        };
        let value = match v.value_type() {
            ValueType::Null => Value::Null,
            ValueType::Integer => Value::Integer(v.to_integer().expect("integer")),
            ValueType::Float => Value::Number(v.to_float().expect("float")),
            ValueType::Text => Value::String(v.to_text().expect("text").into()),
            ValueType::Blob => Value::Binary(v.to_blob().expect("blob")),
            _ => return Err(Error::Validation("unsupported scalar value".into())),
        };
        Ok(ExtValue::from_blob(value.encode()?))
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}
#[scalar(name = "__fastdb_unwrap")]
fn unwrap(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [v] = args else {
            return Err(Error::Validation("unwrap arity".into()));
        };
        scalar_result(&decode_arg(v)?)
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}
#[scalar(name = "__fastdb_helper")]
fn helper(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let (name, args) = args
            .split_first()
            .ok_or_else(|| Error::Validation("helper arity".into()))?;
        let name = name
            .to_text()
            .ok_or_else(|| Error::Validation("helper name".into()))?;
        let args = args.iter().map(decode_arg).collect::<Result<Vec<_>>>()?;
        let value = match (name, args.as_slice()) {
            ("array_new", _) => Value::Array(args),
            ("array_append", [Value::Array(array), element]) => {
                let mut array = array.clone();
                array.push(element.clone());
                Value::Array(array)
            }
            ("record_id", [Value::Record(record)]) => match &record.key {
                crate::Key::Integer(i) => Value::Integer(*i),
                crate::Key::String(s) => Value::String(s.clone()),
            },
            ("record_table", [Value::Record(record)]) => {
                Value::String(record.table.to_ascii_lowercase())
            }
            ("doc_get" | "doc_has", [value, Value::String(path)]) => {
                let found = crate::path::get(value, path)?;
                if name == "doc_has" {
                    Value::Boolean(found.is_some())
                } else {
                    found.cloned().unwrap_or(Value::Null)
                }
            }
            _ => {
                return Err(Error::Validation(
                    "invalid document helper arguments".into(),
                ))
            }
        };
        Ok(ExtValue::from_blob(value.encode()?))
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}

#[scalar(name = "__fastdb_nullable")]
fn nullable(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [v] = args else {
            return Err(Error::Validation("nullable arity".into()));
        };
        let value = decode_arg(v)?;
        if matches!(value, Value::Null) {
            Ok(ExtValue::null())
        } else {
            Ok(ExtValue::from_blob(value.encode()?))
        }
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}

#[scalar(name = "__fastdb_vector_input")]
fn vector_input(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [arg] = args else {
            return Err(Error::Validation("vector input arity".into()));
        };
        match decode_arg(arg)? {
            Value::Vector(bytes) | Value::Binary(bytes) => {
                crate::vectors::dimensions(&bytes)?;
                Ok(ExtValue::from_blob(bytes))
            }
            Value::String(text) => Ok(ExtValue::from_text(text)),
            _ => Err(Error::Validation(
                "vector input requires vector, blob or engine vector text".into(),
            )),
        }
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}
#[scalar(name = "__fastdb_vector_value")]
fn vector_value(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [arg] = args else {
            return Err(Error::Validation("vector constructor arity".into()));
        };
        let bytes = arg
            .to_blob()
            .ok_or_else(|| Error::Storage("vector constructor returned non-blob".into()))?;
        Ok(ExtValue::from_blob(Value::Vector(bytes).encode()?))
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}
