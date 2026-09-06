//! Typed document expressions. Scalar work is delegated to the pinned engine.
use crate::{Connection, Document, Error, Key, Parameters, Record, Result, Value};
use fastql_parser::Expr;
use std::num::NonZeroUsize;
impl Connection {
    pub(crate) fn evaluate(
        &self,
        expr: Expr,
        params: &Parameters,
        doc: Option<&Document>,
    ) -> Result<Value> {
        let value = match expr {
            Expr::Null => Value::Null,
            Expr::Boolean(v) => Value::Boolean(v),
            Expr::Integer(v) => Value::Integer(v),
            Expr::Number(v) => Value::Number(v),
            Expr::String(v) => Value::String(v),
            Expr::Record(v) => Value::Record(v),
            Expr::Parameter(name) => params.get(&name).cloned().ok_or(Error::Parameter(name))?,
            Expr::Field(path) => {
                let doc = doc.ok_or_else(|| {
                    Error::Validation(format!("field {} has no source document", path.join(".")))
                })?;
                match crate::path_value(doc, &path) {
                    Ok(v) => v.cloned().unwrap_or(Value::Null),
                    Err(Error::Validation(_)) => Value::Null,
                    Err(e) => return Err(e),
                }
            }
            Expr::Object(fields) => Value::Object(
                fields
                    .into_iter()
                    .map(|(k, e)| Ok((k, self.evaluate(e, params, doc)?)))
                    .collect::<Result<_>>()?,
            ),
            Expr::Array(values) => Value::Array(
                values
                    .into_iter()
                    .map(|e| self.evaluate(e, params, doc))
                    .collect::<Result<_>>()?,
            ),
            Expr::Unary(op, expr) => {
                let value = self.evaluate(*expr, params, doc)?;
                if !matches!(op.as_str(), "+" | "-" | "NOT") {
                    return Err(Error::Unsupported("unary expression".into()));
                }
                self.scalar_expression(&format!("{op} ?1"), &[value])?
            }
            Expr::Binary(a, op, b) => {
                if !matches!(
                    op.as_str(),
                    "+" | "-"
                        | "*"
                        | "/"
                        | "%"
                        | "||"
                        | "&"
                        | "|"
                        | "<<"
                        | ">>"
                        | "="
                        | "=="
                        | "!="
                        | "<>"
                        | "<"
                        | "<="
                        | ">"
                        | ">="
                        | "IS"
                        | "IS NOT"
                        | "AND"
                        | "OR"
                ) {
                    return Err(Error::Unsupported("binary expression".into()));
                }
                let a = self.evaluate(*a, params, doc)?;
                let b = self.evaluate(*b, params, doc)?;
                self.binary_document(a, &op, b)?
            }
            Expr::Call(name, args) => {
                if name.eq_ignore_ascii_case("coalesce") || name.eq_ignore_ascii_case("ifnull") {
                    if args.len() < 2 || (name.eq_ignore_ascii_case("ifnull") && args.len() != 2) {
                        return Err(Error::Validation("invalid null-helper arity".into()));
                    }
                    let mut result = Value::Null;
                    for expr in args {
                        result = self.evaluate(expr, params, doc)?;
                        if !matches!(result, Value::Null) {
                            break;
                        }
                    }
                    result
                } else {
                    let args = args
                        .into_iter()
                        .map(|e| self.evaluate(e, params, doc))
                        .collect::<Result<Vec<_>>>()?;
                    self.call_document_function(&name, args)?
                }
            }
        };
        value.validate()?;
        Ok(value)
    }
    fn scalar_expression(&self, sql: &str, values: &[Value]) -> Result<Value> {
        if values.iter().any(|v| {
            matches!(
                v,
                Value::Record(_) | Value::Object(_) | Value::Array(_) | Value::Vector(_)
            )
        }) {
            return Err(Error::Validation(
                "this scalar operation requires SQL scalar values".into(),
            ));
        }
        let mut statement = self.engine.prepare(format!("SELECT {sql}"))?;
        for (i, value) in values.iter().enumerate() {
            statement.bind_at(
                NonZeroUsize::new(i + 1).expect("one-based binding"),
                crate::scalar(value)?,
            )?;
        }
        let rows = statement.run_collect_rows()?;
        let value = rows
            .into_iter()
            .next()
            .and_then(|row| row.into_iter().next())
            .ok_or_else(|| Error::Storage("expression returned no value".into()))?;
        Ok(crate::from_engine(value))
    }
    fn binary_document(&self, a: Value, op: &str, b: Value) -> Result<Value> {
        if matches!(&a, Value::Record(_)) || matches!(&b, Value::Record(_)) {
            if !matches!(
                op,
                "=" | "==" | "<>" | "!=" | "IS" | "IS NOT" | "<" | "<=" | ">" | ">="
            ) {
                return Err(Error::Validation("record arithmetic is unsupported".into()));
            }
            if matches!(&a, Value::Null) || matches!(&b, Value::Null) {
                return Ok(match op {
                    "IS" => Value::Integer(0),
                    "IS NOT" => Value::Integer(1),
                    _ => Value::Null,
                });
            }
            let cmp = match (&a, &b) {
                (Value::Record(a), Value::Record(b)) => {
                    let table = a
                        .table
                        .to_ascii_lowercase()
                        .cmp(&b.table.to_ascii_lowercase());
                    table.then_with(|| match (&a.key, &b.key) {
                        (Key::Integer(a), Key::Integer(b)) => a.cmp(b),
                        (Key::String(a), Key::String(b)) => a.cmp(b),
                        (Key::Integer(_), Key::String(_)) => std::cmp::Ordering::Less,
                        _ => std::cmp::Ordering::Greater,
                    })
                }
                _ => {
                    return match op {
                        "=" | "==" | "IS" => Ok(Value::Integer(0)),
                        "<>" | "!=" | "IS NOT" => Ok(Value::Integer(1)),
                        _ => Err(Error::Validation(
                            "mixed record/scalar ordering is unsupported".into(),
                        )),
                    }
                }
            };
            let cmp = match cmp {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            return self.scalar_expression(
                &format!("?1 {op} ?2"),
                &[Value::Integer(cmp), Value::Integer(0)],
            );
        }
        self.scalar_expression(&format!("?1 {op} ?2"), &[a, b])
    }
    fn call_document_function(&self, name: &str, mut args: Vec<Value>) -> Result<Value> {
        match name.to_ascii_lowercase().as_str() {
            "type::record" => match args.as_slice() {
                [Value::String(table), key] => {
                    let key = match key {
                        Value::String(s) => Key::String(s.clone()),
                        Value::Integer(i) => Key::Integer(*i),
                        _ => {
                            return Err(Error::Validation(
                                "record key must be string or int64".into(),
                            ))
                        }
                    };
                    Ok(Value::Record(Record {
                        table: crate::canonical(table)?,
                        key,
                    }))
                }
                _ => Err(Error::Validation(
                    "type::record expects target string and key".into(),
                )),
            },
            "record::id" | "record::table" => match args.as_slice() {
                [Value::Record(r)] => {
                    if name.eq_ignore_ascii_case("record::table") {
                        Ok(Value::String(r.table.to_ascii_lowercase()))
                    } else {
                        Ok(match &r.key {
                            Key::String(s) => Value::String(s.clone()),
                            Key::Integer(i) => Value::Integer(*i),
                        })
                    }
                }
                _ => Err(Error::Validation(
                    "record extractor expects one typed record".into(),
                )),
            },
            "array::new" => Ok(Value::Array(args)),
            "array::append" => {
                if args.len() != 2 {
                    return Err(Error::Validation(
                        "array::append expects array and element".into(),
                    ));
                }
                let element = args.pop().expect("two arguments");
                let Value::Array(mut values) = args.pop().expect("first argument") else {
                    return Err(Error::Validation(
                        "array::append requires an existing array".into(),
                    ));
                };
                values.push(element);
                Ok(Value::Array(values))
            }
            "doc::get" | "doc::has" => match args.as_slice() {
                [value, Value::String(path)] => {
                    let found = crate::path::get(value, path)?;
                    if name.eq_ignore_ascii_case("doc::has") {
                        Ok(Value::Boolean(found.is_some()))
                    } else {
                        Ok(found.cloned().unwrap_or(Value::Null))
                    }
                }
                _ => Err(Error::Validation(
                    "document path function expects value and path string".into(),
                )),
            },
            _ => {
                if name.contains("::") || name.to_ascii_lowercase().starts_with("__fastdb_") {
                    return Err(Error::Unsupported(format!("document function {name}")));
                }
                let args_sql = (1..=args.len())
                    .map(|i| format!("?{i}"))
                    .collect::<Vec<_>>()
                    .join(",");
                self.scalar_expression(&format!("{}({args_sql})", crate::quote(name)), &args)
            }
        }
    }
}
