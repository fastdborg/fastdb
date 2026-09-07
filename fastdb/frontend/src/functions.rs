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
                c"__fastdb_vector_field",
                vector_field as turso_ext::ScalarFunction,
                2,
            ),
            (
                c"__fastdb_vector_concat",
                vector_concat as turso_ext::ScalarFunction,
                2,
            ),
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
                c"__fastdb_pagination_value",
                pagination_value as turso_ext::ScalarFunction,
                1,
            ),
            (c"__fastdb_compare", compare as turso_ext::ScalarFunction, 2),
            (
                c"__fastdb_range_scalar",
                range_scalar as turso_ext::ScalarFunction,
                2,
            ),
            (c"__fastdb_between", between as turso_ext::ScalarFunction, 3),
            (
                c"__fastdb_nullable",
                nullable as turso_ext::ScalarFunction,
                1,
            ),
            (
                c"__fastdb_count_value",
                count_value as turso_ext::ScalarFunction,
                1,
            ),
            (c"__fastdb_unwrap", unwrap as turso_ext::ScalarFunction, 1),
            (
                c"__fastdb_sql_scalar",
                sql_scalar as turso_ext::ScalarFunction,
                1,
            ),
            // Same implementations, distinct expression identities for HAVING
            // on the pinned engine's nonprojected function group keys.
            (
                c"__fastdb_having_sql_scalar",
                sql_scalar as turso_ext::ScalarFunction,
                1,
            ),
            (
                c"__fastdb_having_scalar",
                document_scalar as turso_ext::ScalarFunction,
                2,
            ),
            (c"__fastdb_helper", helper as turso_ext::ScalarFunction, -1),
            (
                c"__fastdb_scalar",
                document_scalar as turso_ext::ScalarFunction,
                2,
            ),
            (
                c"__fastdb_nested_value",
                nested_value as turso_ext::ScalarFunction,
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
                name != c"__fastdb_pagination_value",
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
        return if mode == 4 {
            vector_input_value(Value::Null)
        } else {
            Ok(ExtValue::null())
        };
    }
    let bytes = args[0]
        .to_blob()
        .ok_or_else(|| Error::Storage("expected stored document".into()))?;
    let path: Vec<String> = serde_json::from_str(
        args[1]
            .to_text()
            .ok_or_else(|| Error::Storage("expected field path".into()))?,
    )?;
    let value = match Value::decode(&bytes)? {
        Value::Object(doc) => {
            if path.is_empty() {
                Value::Object(doc)
            } else {
                read_path(&doc, &path)?.cloned().unwrap_or(Value::Null)
            }
        }
        // Derived columns may contain any logical value. A non-object parent
        // has no nested object fields; stored document roots remain strict.
        _ if mode == 3 => Value::Null,
        _ => return Err(Error::Storage("expected object".into())),
    };
    if mode == 4 {
        return vector_input_value(value);
    }
    if mode == 1 || mode == 3 {
        return Ok(ExtValue::from_blob(value.encode()?));
    }
    if mode == 2 {
        return ordered(&value);
    }
    scalar_result(&value)
}
fn ordered(value: &Value) -> Result<ExtValue> {
    if let Value::Binary(bytes) = value {
        let mut key = Vec::with_capacity(bytes.len() + 1);
        key.push(0);
        key.extend(bytes);
        return Ok(ExtValue::from_blob(key));
    }
    if let Value::Record(record) = &value {
        let mut key = vec![1];
        key.extend(record.table.to_ascii_lowercase().into_bytes());
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
#[scalar(name = "__fastdb_nested_value")]
fn nested_value(args: &[ExtValue]) -> ExtValue {
    get(args, 3).unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
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
fn compare_values(a: &Value, b: &Value) -> Result<Option<std::cmp::Ordering>> {
    if matches!(a, Value::Null) || matches!(b, Value::Null) {
        return Ok(None);
    }
    let order = match (a, b) {
        (Value::Binary(a), Value::Binary(b)) => a.cmp(b),
        (Value::Record(a), Value::Record(b)) => a
            .table
            .to_ascii_lowercase()
            .cmp(&b.table.to_ascii_lowercase())
            .then_with(|| match (&a.key, &b.key) {
                (crate::Key::Integer(a), crate::Key::Integer(b)) => a.cmp(b),
                (crate::Key::String(a), crate::Key::String(b)) => a.cmp(b),
                (crate::Key::Integer(_), crate::Key::String(_)) => std::cmp::Ordering::Less,
                (crate::Key::String(_), crate::Key::Integer(_)) => std::cmp::Ordering::Greater,
            }),
        (Value::Record(_), _) | (_, Value::Record(_)) => {
            return Err(Error::Validation(
                "mixed record/scalar ordering is unsupported".into(),
            ))
        }
        _ => index_scalar(a)?.cmp(&index_scalar(b)?),
    };
    Ok(Some(order))
}

#[scalar(name = "__fastdb_between")]
fn between(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [value, start, end] = args else {
            return Err(Error::Validation("BETWEEN arity".into()));
        };
        let (value, start, end) = (decode_arg(value)?, decode_arg(start)?, decode_arg(end)?);
        let lower = compare_values(&value, &start)?.map(|order| !order.is_lt());
        let upper = compare_values(&value, &end)?.map(|order| !order.is_gt());
        // SQL three-valued AND: false dominates null, otherwise unknown stays
        // unknown. Each argument is evaluated exactly once by the engine.
        Ok(match (lower, upper) {
            (Some(false), _) | (_, Some(false)) => ExtValue::from_integer(0),
            (Some(true), Some(true)) => ExtValue::from_integer(1),
            _ => ExtValue::null(),
        })
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}

#[scalar(name = "__fastdb_compare")]
fn compare(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [a, b] = args else {
            return Err(Error::Validation("comparison arity".into()));
        };
        let (a, b) = (decode_arg(a)?, decode_arg(b)?);
        let Some(order) = compare_values(&a, &b)? else {
            return Ok(ExtValue::null());
        };
        Ok(ExtValue::from_integer(match order {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }))
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}

// Native SQL functions and casts consume binary payloads, while predicates
// retain tagged binary keys to prevent collisions with record identities.
// Keep the mutable pagination counter out of the engine's constant registers.
// Preserve raw SQL values so MustBeInt retains responsibility for conversion.
#[scalar(name = "__fastdb_pagination_value")]
fn pagination_value(args: &[ExtValue]) -> ExtValue {
    let [value] = args else {
        return ExtValue::error_with_message("pagination arity".into());
    };
    match value.value_type() {
        ValueType::Null => ExtValue::null(),
        ValueType::Integer => ExtValue::from_integer(value.to_integer().unwrap()),
        ValueType::Float => ExtValue::from_float(value.to_float().unwrap()),
        ValueType::Text => ExtValue::from_text(value.to_text().unwrap().to_owned()),
        ValueType::Blob => ExtValue::from_blob(value.to_blob().unwrap()),
        ValueType::Error => ExtValue::error_with_message("pagination argument error".into()),
    }
}

#[scalar(name = "__fastdb_sql_scalar")]
fn sql_scalar(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [value] = args else {
            return Err(Error::Validation("SQL scalar arity".into()));
        };
        match decode_arg(value)? {
            Value::Binary(bytes) => Ok(ExtValue::from_blob(bytes)),
            value => scalar_result(&value),
        }
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}

// A native column retains its engine affinity/collation in the comparison.
// Convert only the logical operand, without allowing encoded record bytes to
// acquire an accidental order relative to ordinary SQL values.
#[scalar(name = "__fastdb_range_scalar")]
fn range_scalar(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [value, native] = args else {
            return Err(Error::Validation("range scalar arity".into()));
        };
        let value = decode_arg(value)?;
        if native.value_type() == ValueType::Null || matches!(value, Value::Null) {
            return Ok(ExtValue::null());
        }
        match value {
            Value::Record(_) => Err(Error::Validation(
                "mixed record/scalar ordering is unsupported".into(),
            )),
            Value::Binary(bytes) => Ok(ExtValue::from_blob(bytes)),
            value => scalar_result(&value),
        }
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
            ("string_slugify", _) => crate::bundled::call("slugify", &args)?,
            ("string_normalize", _) => crate::bundled::call("normalize", &args)?,
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

#[scalar(name = "__fastdb_count_value")]
fn count_value(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [value] = args else {
            return Err(Error::Validation("count value arity".into()));
        };
        // Validate the full value, but COUNT only needs null presence. Avoid
        // serializing a potentially large composite back into a result blob.
        if matches!(decode_arg(value)?, Value::Null) {
            Ok(ExtValue::null())
        } else {
            Ok(ExtValue::from_integer(1))
        }
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

#[scalar(name = "__fastdb_vector_field")]
fn vector_field(args: &[ExtValue]) -> ExtValue {
    get(args, 4).unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}

fn vector_input_value(value: Value) -> Result<ExtValue> {
    match value {
        Value::Vector(bytes) | Value::Binary(bytes) => {
            crate::vectors::dimensions(&bytes)?;
            Ok(ExtValue::from_blob(bytes))
        }
        Value::String(text) => Ok(ExtValue::from_text(text)),
        _ => Err(Error::Validation(
            "vector input requires vector, blob or engine vector text".into(),
        )),
    }
}

#[scalar(name = "__fastdb_vector_input")]
fn vector_input(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [arg] = args else {
            return Err(Error::Validation("vector input arity".into()));
        };
        vector_input_value(decode_arg(arg)?)
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

#[scalar(name = "__fastdb_vector_concat")]
fn vector_concat(args: &[ExtValue]) -> ExtValue {
    let result = (|| -> Result<ExtValue> {
        let [a, b] = args else {
            return Err(Error::Validation("vector concat arity".into()));
        };
        let input = |v: &ExtValue| -> Result<turso_core::Value> {
            if let Some(bytes) = v.to_blob() {
                crate::vectors::dimensions(&bytes)?;
                return Ok(turso_core::Value::Blob(bytes));
            }
            if let Some(text) = v.to_text() {
                return Ok(crate::text(text));
            }
            Err(Error::Validation("invalid vector concat input".into()))
        };
        let turso_core::Value::Blob(bytes) = crate::vectors::concat(input(a)?, input(b)?)? else {
            unreachable!("vector result");
        };
        Ok(ExtValue::from_blob(bytes))
    })();
    result.unwrap_or_else(|e| ExtValue::error_with_message(e.to_string()))
}

#[cfg(test)]
mod between_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);

    #[scalar(name = "between_tick")]
    fn tick(_: &[ExtValue]) -> ExtValue {
        CALLS.fetch_add(1, Ordering::SeqCst);
        ExtValue::from_integer(7)
    }

    #[test]
    fn typed_between_evaluates_volatile_left_operand_once() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        // Test-only scalar counts actual engine evaluation, without performing
        // database work inside its callback.
        unsafe {
            let api = c.engine._build_turso_ext();
            let code = (api.register_scalar_function)(
                api.ctx,
                c"between_tick".as_ptr(),
                0,
                false,
                0,
                tick,
                None,
                None,
            );
            c.engine._free_extension_ctx(api);
            assert_eq!(code, ResultCode::OK);
        }
        for negate in ["", "NOT "] {
            CALLS.store(0, Ordering::SeqCst);
            let rows=c.execute(&format!("SELECT type::record('docs',between_tick()) {negate}BETWEEN docs:2 AND docs:10 AS value"), &crate::Parameters::new()).unwrap().rows;
            assert_eq!(
                rows,
                vec![vec![Value::Integer(i64::from(negate.is_empty()))]]
            );
            assert_eq!(CALLS.load(Ordering::SeqCst), 1);
        }
        CALLS.store(0, Ordering::SeqCst);
        let rows = c.execute("WITH v(id) AS MATERIALIZED (VALUES (type::record('docs',between_tick())),(type::record('docs',between_tick()))) SELECT record::id(a.id),record::id(b.id) FROM v a JOIN v b ON 1", &crate::Parameters::new()).unwrap().rows;
        assert_eq!(rows, vec![vec![Value::Integer(7), Value::Integer(7)]; 4]);
        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
        CALLS.store(0, Ordering::SeqCst);
        let rows = c.execute("WITH v(id) AS MATERIALIZED (VALUES (type::record('docs',between_tick())),(type::record('docs',between_tick()))) SELECT record::id(id) FROM v UNION ALL SELECT record::id(id) FROM v", &crate::Parameters::new()).unwrap().rows;
        assert_eq!(rows, vec![vec![Value::Integer(7)]; 4]);
        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
        c.execute("CREATE TABLE scalar_inputs", &crate::Parameters::new())
            .unwrap();
        c.execute("INSERT INTO scalar_inputs {n:1}", &crate::Parameters::new())
            .unwrap();
        c.execute("INSERT INTO scalar_inputs {n:2}", &crate::Parameters::new())
            .unwrap();
        CALLS.store(0, Ordering::SeqCst);
        let rows = c
            .execute(
                "SELECT (SELECT between_tick() FROM scalar_inputs) AS v FROM scalar_inputs",
                &crate::Parameters::new(),
            )
            .unwrap()
            .rows;
        assert_eq!(rows, vec![vec![Value::Integer(7)]; 2]);
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
        CALLS.store(0, Ordering::SeqCst);
        let rows = c.execute("SELECT CASE WHEN 0 THEN (SELECT between_tick() FROM scalar_inputs) ELSE 3 END AS v", &crate::Parameters::new()).unwrap().rows;
        assert_eq!(rows, vec![vec![Value::Integer(3)]]);
        let logical_calls = CALLS.load(Ordering::SeqCst);
        CALLS.store(0, Ordering::SeqCst);
        let rows = c
            .execute(
                "SELECT CASE WHEN 0 THEN (SELECT between_tick()) ELSE 3 END AS v",
                &crate::Parameters::new(),
            )
            .unwrap()
            .rows;
        assert_eq!(rows, vec![vec![Value::Integer(3)]]);
        assert_eq!(CALLS.load(Ordering::SeqCst), logical_calls);
        assert_eq!(logical_calls, 1);
        for (source, expected) in [
            ("SELECT n FROM scalar_inputs", 2),
            ("SELECT DISTINCT n FROM scalar_inputs", 2),
            (
                "SELECT n FROM scalar_inputs UNION ALL SELECT n FROM scalar_inputs",
                4,
            ),
            (
                "SELECT n FROM scalar_inputs UNION SELECT n FROM scalar_inputs",
                2,
            ),
        ] {
            CALLS.store(0, Ordering::SeqCst);
            let rows = c.execute(
                &format!("{source} LIMIT (SELECT between_tick() FROM scalar_inputs) OFFSET (SELECT between_tick()-7 FROM scalar_inputs)"),
                &crate::Parameters::new(),
            ).unwrap().rows;
            assert_eq!(rows.len(), expected, "{source}");
            assert_eq!(CALLS.load(Ordering::SeqCst), 2, "{source}");
        }
        c.execute(
            "CREATE TABLE exists_native(n INTEGER)",
            &crate::Parameters::new(),
        )
        .unwrap();
        c.execute(
            "INSERT INTO exists_native VALUES (1),(2)",
            &crate::Parameters::new(),
        )
        .unwrap();
        for comparison in [
            "n=(SELECT between_tick() FROM exists_native)",
            "(SELECT between_tick() FROM exists_native)>n",
            "n=((SELECT between_tick() FROM exists_native) COLLATE NOCASE)",
        ] {
            CALLS.store(0, Ordering::SeqCst);
            c.execute(
                &format!("SELECT {comparison} AS v FROM scalar_inputs"),
                &crate::Parameters::new(),
            )
            .unwrap();
            assert_eq!(CALLS.load(Ordering::SeqCst), 1, "{comparison}");
        }
        CALLS.store(0, Ordering::SeqCst);
        let rows = c.execute("SELECT EXISTS (SELECT between_tick() FROM exists_native) AS present FROM scalar_inputs", &crate::Parameters::new()).unwrap().rows;
        assert_eq!(rows, vec![vec![Value::Integer(1)]; 2]);
        assert_eq!(CALLS.load(Ordering::SeqCst), 0);
        for inner in [
            "between_tick() FROM scalar_inputs",
            "n FROM scalar_inputs WHERE between_tick()=7",
        ] {
            CALLS.store(0, Ordering::SeqCst);
            let rows = c
                .execute(
                    &format!("SELECT EXISTS (SELECT {inner}) AS v"),
                    &crate::Parameters::new(),
                )
                .unwrap()
                .rows;
            assert_eq!(rows, vec![vec![Value::Integer(1)]]);
            let logical_calls = CALLS.load(Ordering::SeqCst);
            CALLS.store(0, Ordering::SeqCst);
            let native_inner = if inner.starts_with("between_tick") {
                "between_tick()"
            } else {
                "1 WHERE between_tick()=7"
            };
            let native = c
                .execute(
                    &format!("SELECT EXISTS (SELECT {native_inner}) AS v"),
                    &crate::Parameters::new(),
                )
                .unwrap()
                .rows;
            assert_eq!(rows, native);
            assert_eq!(CALLS.load(Ordering::SeqCst), logical_calls);
            assert_eq!(
                logical_calls,
                usize::from(!inner.starts_with("between_tick"))
            );
        }
        c.execute(
            "CREATE TABLE membership_native(n INTEGER)",
            &crate::Parameters::new(),
        )
        .unwrap();
        c.execute(
            "INSERT INTO membership_native VALUES (7),(8)",
            &crate::Parameters::new(),
        )
        .unwrap();
        CALLS.store(0, Ordering::SeqCst);
        let rows = c.execute("SELECT n IN (SELECT between_tick() FROM scalar_inputs) AS v FROM membership_native ORDER BY n", &crate::Parameters::new()).unwrap().rows;
        assert_eq!(rows, vec![vec![Value::Integer(1)], vec![Value::Integer(0)]]);
        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
        for outer in ["membership_native", "scalar_inputs"] {
            for not in ["", "NOT "] {
                CALLS.store(0, Ordering::SeqCst);
                let rows = c.execute(&format!("SELECT n {not}IN (SELECT between_tick() FROM membership_native) FROM {outer}"), &crate::Parameters::new()).unwrap().rows;
                assert_eq!(rows.len(), 2);
                assert_eq!(
                    CALLS.load(Ordering::SeqCst),
                    2,
                    "native membership source calls: {outer}, {not}"
                );
            }
        }
        for outer in ["membership_native", "scalar_inputs"] {
            CALLS.store(0, Ordering::SeqCst);
            let rows = c.execute(&format!("SELECT (between_tick()+n) IN (SELECT n FROM membership_native) FROM {outer}"), &crate::Parameters::new()).unwrap().rows;
            assert_eq!(rows.len(), 2);
            assert_eq!(
                CALLS.load(Ordering::SeqCst),
                2,
                "membership LHS calls: {outer}"
            );
        }
        for (operator, count) in [("UNION", 1), ("INTERSECT", 1), ("EXCEPT", 0)] {
            CALLS.store(0, Ordering::SeqCst);
            let rows = c.execute(&format!("SELECT type::record('docs',between_tick()) AS id {operator} SELECT type::record('docs',between_tick())"), &crate::Parameters::new()).unwrap().rows;
            assert_eq!(rows.len(), count);
            assert_eq!(CALLS.load(Ordering::SeqCst), 2);
        }
        c.execute(
            "CREATE TABLE native_between(value INTEGER)",
            &crate::Parameters::new(),
        )
        .unwrap();
        c.execute(
            "INSERT INTO native_between VALUES (7)",
            &crate::Parameters::new(),
        )
        .unwrap();
        for negate in ["", "NOT "] {
            CALLS.store(0, Ordering::SeqCst);
            let rows = c.execute(&format!(
                "SELECT record::id(type::record('docs',between_tick())) {negate}BETWEEN value AND value FROM native_between"
            ), &crate::Parameters::new()).unwrap().rows;
            assert_eq!(
                rows,
                vec![vec![Value::Integer(i64::from(negate.is_empty()))]]
            );
            assert_eq!(CALLS.load(Ordering::SeqCst), 1);
        }
        for negate in ["", "NOT "] {
            for (lower, upper, calls) in [
                ("record::id(type::record('docs',between_tick()))", "10", 1),
                ("1", "record::id(type::record('docs',between_tick()))", 1),
                (
                    "record::id(type::record('docs',between_tick()))",
                    "record::id(type::record('docs',between_tick()))",
                    2,
                ),
            ] {
                CALLS.store(0, Ordering::SeqCst);
                let rows = c
                    .execute(
                        &format!(
                            "SELECT value {negate}BETWEEN {lower} AND {upper} FROM native_between"
                        ),
                        &crate::Parameters::new(),
                    )
                    .unwrap()
                    .rows;
                assert_eq!(
                    rows,
                    vec![vec![Value::Integer(i64::from(negate.is_empty()))]]
                );
                assert_eq!(CALLS.load(Ordering::SeqCst), calls);
            }
        }
        c.execute(
            "CREATE TABLE correlation_inputs(n INTEGER)",
            &crate::Parameters::new(),
        )
        .unwrap();
        c.execute(
            "INSERT INTO correlation_inputs VALUES(1),(2)",
            &crate::Parameters::new(),
        )
        .unwrap();
        for (projection, expected_calls) in [
            ("(SELECT DISTINCT CASE WHEN d.n>0 THEN between_tick() ELSE d.n END AS x FROM correlation_inputs ORDER BY x DESC,n LIMIT 0)", 0),
            ("(SELECT DISTINCT CASE WHEN d.n>0 THEN between_tick() ELSE d.n END AS x FROM correlation_inputs ORDER BY x DESC,n LIMIT 1)", 4),
            ("(SELECT DISTINCT CASE WHEN d.n>0 THEN 1 ELSE d.n END AS x FROM correlation_inputs ORDER BY x DESC,between_tick() LIMIT 1)", 4),
            ("(SELECT CASE WHEN d.n>0 THEN between_tick() ELSE d.n END AS x FROM correlation_inputs ORDER BY x+0 DESC LIMIT 1)", 8),
            ("(SELECT CASE WHEN d.n>0 THEN between_tick() ELSE d.n END AS x FROM correlation_inputs ORDER BY x DESC LIMIT 0)", 0),
            ("(SELECT CASE WHEN d.n>0 THEN between_tick() ELSE d.n END AS x FROM correlation_inputs ORDER BY x DESC,n LIMIT 1)", 4),
            ("(SELECT CASE WHEN d.n>0 THEN between_tick() ELSE d.n END AS x FROM correlation_inputs ORDER BY x DESC LIMIT 1)", 4),
            (
                "(SELECT between_tick()+d.n AS x FROM correlation_inputs ORDER BY x DESC LIMIT 1)",
                4,
            ),
            ("(SELECT between_tick() WHERE d.n>0)", 2),
            (
                "d.n IN (SELECT between_tick() FROM correlation_inputs WHERE n<=d.n)",
                3,
            ),
            (
                "d.n NOT IN (SELECT between_tick() FROM correlation_inputs WHERE n<=d.n)",
                3,
            ),
            ("(SELECT between_tick() WHERE d.n<2)", 1),
            ("(SELECT between_tick() WHERE d.n<0)", 0),
            ("(SELECT between_tick() WHERE d.n>0)+1", 2),
            ("(SELECT between_tick() WHERE d.n>0)=d.n", 2),
            ("d.n<(SELECT between_tick() WHERE d.n>0)", 2),
            (
                "(SELECT between_tick() WHERE d.n>0)=(SELECT between_tick() WHERE d.n>0)",
                4,
            ),
        ] {
            let native = format!("SELECT {projection} FROM correlation_inputs AS d ORDER BY d.n");
            CALLS.store(0, Ordering::SeqCst);
            let expected = c.execute(&native, &crate::Parameters::new()).unwrap().rows;
            assert_eq!(
                CALLS.load(Ordering::SeqCst),
                expected_calls,
                "native: {projection}"
            );
            let sql = native.replace("FROM correlation_inputs AS d", "FROM scalar_inputs AS d");
            for profile in [false, true] {
                CALLS.store(0, Ordering::SeqCst);
                let actual = if profile {
                    c.profile_select(&sql, &crate::Parameters::new())
                        .unwrap()
                        .result
                        .rows
                } else {
                    c.execute(&sql, &crate::Parameters::new()).unwrap().rows
                };
                assert_eq!(actual, expected, "{sql}, profile={profile}");
                assert_eq!(
                    CALLS.load(Ordering::SeqCst),
                    expected_calls,
                    "{sql}, profile={profile}"
                );
            }
        }
    }
}

#[cfg(test)]
mod nested_accessor_tests {
    use super::*;
    #[test]
    fn derived_accessors_allow_scalar_parents_without_weakening_stored_roots() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        for bytes in [
            Value::Integer(7).encode().unwrap(),
            Value::Null.encode().unwrap(),
            Value::Array(vec![]).encode().unwrap(),
            vec![0xff],
        ] {
            for function in ["__fastdb_nested_value", "__fastdb_value", "__fastdb_scalar"] {
                let mut statement = c
                    .prepare(format!("SELECT {function}(?1,'[\"n\"]')"))
                    .unwrap();
                statement
                    .bind_at(
                        std::num::NonZeroUsize::new(1).unwrap(),
                        turso_core::Value::Blob(bytes.clone()),
                    )
                    .unwrap();
                let result = crate::collect_rows(&mut statement);
                if function == "__fastdb_nested_value" && bytes != [0xff] {
                    let rows = result.unwrap();
                    let turso_core::Value::Blob(value) = &rows[0][0] else {
                        panic!("typed nested value")
                    };
                    assert_eq!(Value::decode(value).unwrap(), Value::Null);
                } else {
                    assert!(result.is_err(), "{function}, {bytes:?}");
                }
            }
        }
    }
}

#[cfg(test)]
mod vector_field_tests {
    use super::*;
    #[test]
    fn fused_vector_errors_preserve_native_transaction_disposition() {
        for input in [
            turso_core::Value::Null,
            turso_core::Value::Blob(vec![255]),
            turso_core::Value::Blob(
                Value::Object(crate::Document::from([("v".into(), Value::Null)]))
                    .encode()
                    .unwrap(),
            ),
        ] {
            for outer in [false, true] {
                let run = |expression: &str| {
                    let db = crate::Database::open(":memory:").unwrap();
                    let c = db.connect().unwrap();
                    let sql = |sql: &str| c.execute(sql, &crate::Parameters::new()).unwrap();
                    sql("CREATE TABLE prior(n INTEGER)");
                    sql("INSERT INTO prior VALUES(1)");
                    if outer {
                        sql("BEGIN");
                        sql("INSERT INTO prior VALUES(2)");
                    }
                    let mut statement = c.prepare(format!("SELECT {expression}")).unwrap();
                    statement
                        .bind_at(std::num::NonZeroUsize::new(1).unwrap(), input.clone())
                        .unwrap();
                    let error = crate::collect_rows(&mut statement).unwrap_err().to_string();
                    drop(statement);
                    let state = c.transaction_state();
                    let rows = sql("SELECT n FROM prior ORDER BY n").rows;
                    assert_eq!(state, crate::TransactionState::Autocommit);
                    assert_eq!(rows, vec![vec![Value::Integer(1)]]);
                    sql("INSERT INTO prior VALUES(3)");
                    assert_eq!(
                        sql("SELECT count(*) FROM prior").rows,
                        vec![vec![Value::Integer(2)]]
                    );
                    (error, state, rows)
                };
                assert_eq!(
                    run("__fastdb_vector_field(?1,'[\"v\"]')"),
                    run("__fastdb_vector_input(__fastdb_value(?1,'[\"v\"]'))")
                );
            }
        }
    }

    #[test]
    fn fused_vector_fields_match_generic_conversion_and_errors() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        let mut values = vec![
            Value::Null,
            Value::Integer(7),
            Value::String("[1,2,3]".into()),
            Value::Binary(vec![255]),
        ];
        for v in [
            Value::vector32(&[1., 2., 3.]),
            Value::vector64(&[1., 2., 3.]),
            Value::vector32_sparse(&[1., 0., 3.]),
            Value::vector8(&[1., 2., 3.]),
            Value::vector1bit(&[1., -2., 3.]),
        ] {
            let v = v.unwrap();
            if let Value::Vector(bytes) = &v {
                values.push(Value::Binary(bytes.clone()));
            }
            values.push(v);
        }
        let mut inputs = vec![
            turso_core::Value::Null,
            turso_core::Value::Blob(vec![255]),
            turso_core::Value::Blob(Value::Integer(1).encode().unwrap()),
        ];
        for value in values {
            inputs.push(turso_core::Value::Blob(
                Value::Object(crate::Document::from([("v".into(), value)]))
                    .encode()
                    .unwrap(),
            ));
        }
        inputs.push(turso_core::Value::Blob(b"FDB\x01{\"type\":\"Object\",\"value\":{\"v\":{\"type\":\"String\",\"value\":\"[1,2,3]\"},\"bad\":{\"type\":\"Vector\",\"value\":[255]}}}".to_vec()));
        for input in inputs {
            for path in ["[\"v\"]", "[\"missing\"]", "[\"v\",\"nested\"]", "[]"] {
                let run = |expr: &str| {
                    let mut statement = c.prepare(format!("SELECT {expr}")).unwrap();
                    statement
                        .bind_at(std::num::NonZeroUsize::new(1).unwrap(), input.clone())
                        .unwrap();
                    crate::collect_rows(&mut statement).map_err(|e| e.to_string())
                };
                assert_eq!(
                    run(&format!("__fastdb_vector_field(?1,'{path}')")),
                    run(&format!(
                        "__fastdb_vector_input(__fastdb_value(?1,'{path}'))"
                    )),
                    "{input:?}, {path}"
                );
            }
        }
    }
}

#[cfg(test)]
mod count_value_tests {
    use super::*;
    use crate::EngineValue;
    #[test]
    fn count_marker_validates_encoded_values_and_returns_only_presence() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        for (value, expected) in [
            (Value::Null, EngineValue::Null),
            (Value::Boolean(false), EngineValue::from_i64(1)),
            (
                Value::Array(vec![Value::String("x".repeat(65536))]),
                EngineValue::from_i64(1),
            ),
        ] {
            let result = c
                .run(
                    "SELECT __fastdb_count_value(?1)",
                    &[EngineValue::Blob(value.encode().unwrap())],
                )
                .unwrap();
            assert_eq!(result, vec![vec![expected]]);
        }
        let mut invalid_vector = b"FDB\x01".to_vec();
        invalid_vector.extend(serde_json::to_vec(&Value::Vector(vec![])).unwrap());
        for bytes in [vec![], b"FDB\x01{".to_vec(), invalid_vector] {
            assert!(c
                .run(
                    "SELECT __fastdb_count_value(?1)",
                    &[EngineValue::Blob(bytes)]
                )
                .is_err());
        }
        assert!(c
            .run("SELECT __fastdb_count_value('not encoded')", &[])
            .is_err());
        assert_eq!(
            c.run("SELECT __fastdb_count_value(NULL)", &[]).unwrap(),
            vec![vec![EngineValue::Null]]
        );
    }
}

#[cfg(test)]
mod grouped_evaluation_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);

    #[scalar(name = "grouped_tick")]
    fn grouped_tick(_: &[ExtValue]) -> ExtValue {
        CALLS.fetch_add(1, Ordering::SeqCst);
        ExtValue::from_integer(1)
    }

    #[test]
    fn projected_aggregate_aliases_do_not_repeat_volatile_arguments() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        unsafe {
            let api = c.engine._build_turso_ext();
            let code = (api.register_scalar_function)(
                api.ctx,
                c"grouped_tick".as_ptr(),
                0,
                false,
                0,
                grouped_tick,
                None,
                None,
            );
            c.engine._free_extension_ctx(api);
            assert_eq!(code, ResultCode::OK);
        }
        let params = crate::Parameters::new();
        for sql in [
            "CREATE TABLE docs",
            "CREATE TABLE baseline(k,n)",
            "INSERT INTO docs {k:'a',n:1}",
            "INSERT INTO docs {k:'a',n:2}",
            "INSERT INTO docs {k:'b',n:4}",
            "INSERT INTO baseline VALUES('a',1),('a',2),('b',4)",
        ] {
            c.execute(sql, &params).unwrap();
        }
        for output in [
            "sum(n+grouped_tick())",
            "sum(n+grouped_tick())+1",
            "SUM(n+grouped_tick())",
        ] {
            for having in ["total>0", "sum(n+grouped_tick())>0"] {
                for order in ["total", "sum(n+grouped_tick())"] {
                    let sql = |table| {
                        format!("SELECT k,{output} AS total FROM {table} GROUP BY k HAVING {having} ORDER BY {order},k")
                    };
                    CALLS.store(0, Ordering::SeqCst);
                    let expected = c.execute(&sql("baseline"), &params).unwrap().rows;
                    assert_eq!(
                        CALLS.load(Ordering::SeqCst),
                        3,
                        "native {output}: {having}: {order}"
                    );
                    CALLS.store(0, Ordering::SeqCst);
                    assert_eq!(c.execute(&sql("docs"), &params).unwrap().rows, expected);
                    assert_eq!(
                        CALLS.load(Ordering::SeqCst),
                        3,
                        "{output}: {having}: {order}"
                    );
                    CALLS.store(0, Ordering::SeqCst);
                    assert_eq!(
                        c.profile_select(&sql("docs"), &params).unwrap().result.rows,
                        expected
                    );
                    assert_eq!(
                        CALLS.load(Ordering::SeqCst),
                        3,
                        "profile {output}: {having}: {order}"
                    );
                }
            }
        }
        for (minimum, calls) in [(1, 2), (10, 0)] {
            let params = crate::Parameters::from([("$minimum".into(), Value::Integer(minimum))]);
            let aggregate = "sum(n+grouped_tick()) FILTER (WHERE n>$minimum)";
            for having in [
                "total IS NOT NULL".to_owned(),
                format!("{aggregate} IS NOT NULL"),
            ] {
                let sql = |table| {
                    format!("SELECT k,{aggregate} AS total FROM {table} GROUP BY k HAVING {having} ORDER BY {aggregate},k")
                };
                CALLS.store(0, Ordering::SeqCst);
                let expected = c.execute(&sql("baseline"), &params).unwrap().rows;
                assert_eq!(
                    CALLS.load(Ordering::SeqCst),
                    calls,
                    "native FILTER {minimum}"
                );
                CALLS.store(0, Ordering::SeqCst);
                assert_eq!(c.execute(&sql("docs"), &params).unwrap().rows, expected);
                assert_eq!(
                    CALLS.load(Ordering::SeqCst),
                    calls,
                    "FILTER {minimum}: {having}"
                );
                CALLS.store(0, Ordering::SeqCst);
                assert_eq!(
                    c.profile_select(&sql("docs"), &params).unwrap().result.rows,
                    expected
                );
                assert_eq!(
                    CALLS.load(Ordering::SeqCst),
                    calls,
                    "profile FILTER {minimum}: {having}"
                );
            }
        }
    }
}

#[cfg(test)]
mod cte_evaluation_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    #[scalar(name = "cte_tick")]
    fn cte_tick(_: &[ExtValue]) -> ExtValue {
        CALLS.fetch_add(1, Ordering::SeqCst);
        ExtValue::from_integer(1)
    }
    #[test]
    fn local_cte_correlation_probes_do_not_execute_sources() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        unsafe {
            let api = c.engine._build_turso_ext();
            let code = (api.register_scalar_function)(
                api.ctx,
                c"cte_tick".as_ptr(),
                0,
                false,
                0,
                cte_tick,
                None,
                None,
            );
            c.engine._free_extension_ctx(api);
            assert_eq!(code, ResultCode::OK);
        }
        let params = crate::Parameters::new();
        for sql in [
            "CREATE TABLE docs",
            "CREATE TABLE baseline(n)",
            "INSERT INTO docs {n:1}",
            "INSERT INTO docs {n:2}",
            "INSERT INTO docs {n:4}",
            "INSERT INTO baseline VALUES(1),(2),(4)",
        ] {
            c.execute(sql, &params).unwrap();
        }
        for materialization in ["MATERIALIZED", "NOT MATERIALIZED", ""] {
            for limit in [" LIMIT 0", ""] {
                let sql = |table| {
                    format!("SELECT n,(WITH x AS {materialization} (SELECT n,cte_tick() AS v FROM {table}) SELECT sum(v) FROM x WHERE x.n>=d.n) FROM {table} d ORDER BY n{limit}")
                };
                CALLS.store(0, Ordering::SeqCst);
                let expected = c.execute(&sql("baseline"), &params).unwrap().rows;
                let expected_calls = CALLS.load(Ordering::SeqCst);
                if !limit.is_empty() {
                    assert_eq!(expected_calls, 0);
                } else {
                    assert!(expected_calls > 0);
                }
                CALLS.store(0, Ordering::SeqCst);
                assert_eq!(c.execute(&sql("docs"), &params).unwrap().rows, expected);
                assert_eq!(
                    CALLS.load(Ordering::SeqCst),
                    expected_calls,
                    "execute {materialization}: {limit}"
                );
                CALLS.store(0, Ordering::SeqCst);
                assert_eq!(
                    c.profile_select(&sql("docs"), &params).unwrap().result.rows,
                    expected
                );
                assert_eq!(
                    CALLS.load(Ordering::SeqCst),
                    expected_calls,
                    "profile {materialization}: {limit}"
                );
            }
        }
        for (projection, predicate, calls) in [("1", " WHERE v>0", 2), ("cte_tick()", "", 0)] {
            for negate in ["", "NOT "] {
                let source_value = if calls == 0 { "1" } else { "cte_tick()" };
                let sql = format!("SELECT n,{negate}EXISTS(WITH x AS NOT MATERIALIZED (SELECT {source_value} AS v FROM docs s WHERE s.n>d.n) SELECT {projection} FROM x{predicate}) FROM docs d ORDER BY n");
                let flag = i64::from(negate.is_empty());
                let expected = vec![
                    vec![Value::Integer(1), Value::Integer(flag)],
                    vec![Value::Integer(2), Value::Integer(flag)],
                    vec![Value::Integer(4), Value::Integer(1 - flag)],
                ];
                CALLS.store(0, Ordering::SeqCst);
                assert_eq!(c.execute(&sql, &params).unwrap().rows, expected);
                assert_eq!(CALLS.load(Ordering::SeqCst), calls, "execute {sql}");
                CALLS.store(0, Ordering::SeqCst);
                assert_eq!(
                    c.profile_select(&sql, &params).unwrap().result.rows,
                    expected
                );
                assert_eq!(CALLS.load(Ordering::SeqCst), calls, "profile {sql}");
            }
        }
        for outer_limit in ["", " LIMIT 0"] {
            for skipped in [false, true] {
                let offset = if skipped {
                    "cte_tick()"
                } else {
                    "cte_tick()-1"
                };
                let sql = format!("SELECT (SELECT array::new(d.n,cte_tick()) LIMIT cte_tick() OFFSET {offset}) FROM docs d ORDER BY n{outer_limit}");
                let expected = if outer_limit.is_empty() {
                    vec![1, 2, 4]
                        .into_iter()
                        .map(|n| {
                            vec![if skipped {
                                Value::Null
                            } else {
                                Value::Array(vec![Value::Integer(n), Value::Integer(1)])
                            }]
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };
                let expected_calls = if outer_limit.is_empty() {
                    if skipped {
                        6
                    } else {
                        9
                    }
                } else {
                    0
                };
                CALLS.store(0, Ordering::SeqCst);
                assert_eq!(c.execute(&sql, &params).unwrap().rows, expected);
                assert_eq!(
                    CALLS.load(Ordering::SeqCst),
                    expected_calls,
                    "execute {sql}"
                );
                CALLS.store(0, Ordering::SeqCst);
                assert_eq!(
                    c.profile_select(&sql, &params).unwrap().result.rows,
                    expected
                );
                assert_eq!(
                    CALLS.load(Ordering::SeqCst),
                    expected_calls,
                    "profile {sql}"
                );
            }
        }
        for offset in [0, 1] {
            for outer_limit in ["", " LIMIT 0"] {
                let reference = format!("SELECT (SELECT array::new(s.n,1) FROM docs s WHERE s.n>=d.n LIMIT 1 OFFSET {offset}) FROM docs d ORDER BY n{outer_limit}");
                let expected = c.execute(&reference, &params).unwrap().rows;
                let offset_expr = if offset == 0 {
                    "cte_tick()-1"
                } else {
                    "cte_tick()"
                };
                let sql = format!("SELECT (SELECT array::new(s.n,cte_tick()) FROM docs s WHERE s.n>=d.n LIMIT cte_tick() OFFSET {offset_expr}) FROM docs d ORDER BY n{outer_limit}");
                let expected_calls = if !outer_limit.is_empty() {
                    0
                } else if offset == 0 {
                    9
                } else {
                    8
                };
                CALLS.store(0, Ordering::SeqCst);
                assert_eq!(c.execute(&sql, &params).unwrap().rows, expected);
                assert_eq!(
                    CALLS.load(Ordering::SeqCst),
                    expected_calls,
                    "execute {sql}"
                );
                CALLS.store(0, Ordering::SeqCst);
                assert_eq!(
                    c.profile_select(&sql, &params).unwrap().result.rows,
                    expected
                );
                assert_eq!(
                    CALLS.load(Ordering::SeqCst),
                    expected_calls,
                    "profile {sql}"
                );
            }
        }
    }
}
