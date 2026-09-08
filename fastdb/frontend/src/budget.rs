use crate::{Error, Key, Result, Value};

/// Explicit limits for a materialized SELECT result. This does not bound engine
/// working memory, decoding allocations, or allocator overhead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResultLimits {
    pub max_rows: usize,
    /// Column names and string/object-key/record-name bytes use UTF-8 lengths.
    /// Null and booleans cost one byte; numbers and integer keys cost eight.
    /// Binary/vector bytes count verbatim; arrays and objects sum their contents.
    pub max_payload_bytes: usize,
}

pub(crate) struct ResultBudget {
    limits: Option<ResultLimits>,
    rows: usize,
    bytes: usize,
}
impl ResultBudget {
    pub(crate) fn new(limits: Option<ResultLimits>, columns: &[String]) -> Result<Self> {
        let mut budget = Self {
            limits,
            rows: 0,
            bytes: 0,
        };
        if limits.is_some() {
            for column in columns {
                budget.add(column.len())?;
            }
        }
        Ok(budget)
    }
    fn add(&mut self, bytes: usize) -> Result<()> {
        self.bytes = self.bytes.checked_add(bytes).ok_or_else(payload_error)?;
        if self.bytes > self.limits.expect("bounded accounting").max_payload_bytes {
            return Err(payload_error());
        }
        Ok(())
    }
    fn value(&mut self, value: &Value) -> Result<()> {
        match value {
            Value::Null | Value::Boolean(_) => self.add(1),
            Value::Integer(_) | Value::Number(_) => self.add(8),
            Value::String(s) => self.add(s.len()),
            Value::Binary(b) | Value::Vector(b) => self.add(b.len()),
            Value::Record(r) => {
                self.add(r.table.len())?;
                self.add(match &r.key {
                    Key::String(s) => s.len(),
                    Key::Integer(_) => 8,
                })
            }
            Value::Array(values) => {
                for value in values {
                    self.value(value)?;
                }
                Ok(())
            }
            Value::Object(fields) => {
                for (key, value) in fields {
                    self.add(key.len())?;
                    self.value(value)?;
                }
                Ok(())
            }
        }
    }
    pub(crate) fn row(&mut self, row: &[Value]) -> Result<()> {
        let Some(limits) = self.limits else {
            return Ok(());
        };
        if self.rows >= limits.max_rows {
            return Err(Error::Limit("SELECT result row limit exceeded".into()));
        }
        self.rows += 1;
        for value in row {
            self.value(value)?;
        }
        Ok(())
    }
}
fn payload_error() -> Error {
    Error::Limit("SELECT result payload limit exceeded".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recursive_payload_and_checked_overflow() {
        let values = vec![Value::Object(
            [(
                "é".into(),
                Value::Array(vec![
                    Value::Null,
                    Value::Boolean(true),
                    Value::Integer(7),
                    Value::Number(1.5),
                    Value::String("猫".into()),
                    Value::Binary(vec![0, 255]),
                    Value::Record(crate::Record {
                        table: "docs".into(),
                        key: Key::String("é".into()),
                    }),
                    Value::Vector(vec![0; 13]),
                ]),
            )]
            .into(),
        )];
        // Column: 2; object key: 2; array contents: 1+1+8+8+3+2+6+13.
        let limits = ResultLimits {
            max_rows: 1,
            max_payload_bytes: 46,
        };
        ResultBudget::new(Some(limits), &["é".into()])
            .unwrap()
            .row(&values)
            .unwrap();
        assert!(ResultBudget::new(
            Some(ResultLimits {
                max_payload_bytes: 45,
                ..limits
            }),
            &["é".into()]
        )
        .unwrap()
        .row(&values)
        .is_err());
        let mut budget = ResultBudget::new(
            Some(ResultLimits {
                max_rows: usize::MAX,
                max_payload_bytes: usize::MAX,
            }),
            &[],
        )
        .unwrap();
        budget.bytes = usize::MAX;
        assert!(matches!(budget.row(&[Value::Null]), Err(Error::Limit(_))));
    }
}
