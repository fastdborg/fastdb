//! Bounded object, array and record-link projection paths.
use crate::{Error, Result, Value};
use fastql_parser::{Kind, Token};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum Part {
    Key(String),
    Index(i64),
    All,
}

pub(crate) fn decode(encoded: &str) -> Result<Vec<Part>> {
    let parts: Vec<Part> = serde_json::from_str(encoded)?;
    if parts.len() > 64 {
        return Err(Error::Limit("projection path nesting exceeds 64".into()));
    }
    Ok(parts)
}

/// Evaluate in waves: collect unresolved references across all rows, fetch each
/// wave in batches, then resume. No database reads run inside engine callbacks.
pub(crate) fn project(
    connection: &crate::Connection,
    inputs: &[(Value, Vec<Part>)],
    budget: &mut crate::budget::ResultBudget,
) -> Result<(Vec<Value>, crate::links::FetchMetrics)> {
    use std::collections::BTreeMap;
    struct Walker {
        output_bytes: crate::links::FetchBudget,
        cache: BTreeMap<String, Value>,
        pending: BTreeMap<String, Value>,
        work: usize,
    }
    impl Walker {
        fn walk(&mut self, value: &Value, parts: &[Part]) -> Result<Value> {
            self.work += 1;
            if self.work > 1_000_000 {
                return Err(Error::Limit(
                    "projection traversal work exceeds 1000000".into(),
                ));
            }
            let Some((part, rest)) = parts.split_first() else {
                self.output_bytes.charge(value)?;
                return Ok(value.clone());
            };
            if let Value::Record(record) = value {
                let mut record = record.clone();
                record.table = crate::canonical(&record.table)?;
                let key = serde_json::to_string(&record)?;
                if let Some(document) = self.cache.get(&key).cloned() {
                    return self.walk(&document, parts);
                }
                self.pending.insert(key, value.clone());
                if self.pending.len() + self.cache.len() > crate::links::MAX_FETCH_REFERENCES {
                    return Err(Error::Limit("projection references exceed 16384".into()));
                }
                return Ok(Value::Null);
            }
            Ok(match (value, part) {
                (Value::Object(object), Part::Key(key)) => {
                    self.walk(object.get(key).unwrap_or(&Value::Null), rest)?
                }
                (Value::Array(array), Part::Index(index)) => {
                    let index = if *index >= 0 {
                        usize::try_from(*index).ok()
                    } else {
                        usize::try_from(index.unsigned_abs())
                            .ok()
                            .and_then(|offset| array.len().checked_sub(offset))
                    };
                    self.walk(
                        index.and_then(|i| array.get(i)).unwrap_or(&Value::Null),
                        rest,
                    )?
                }
                (Value::Object(_), Part::All) => self.walk(value, rest)?,
                (Value::Array(array), Part::All) => Value::Array(
                    array
                        .iter()
                        .map(|value| {
                            self.walk(
                                value,
                                if matches!(value, Value::Record(_)) {
                                    parts
                                } else {
                                    rest
                                },
                            )
                        })
                        .collect::<Result<_>>()?,
                ),
                (Value::Array(array), Part::Key(_)) => Value::Array(
                    array
                        .iter()
                        .map(|value| self.walk(value, parts))
                        .collect::<Result<_>>()?,
                ),
                _ => Value::Null,
            })
        }
    }
    let mut walker = Walker {
        output_bytes: crate::links::FetchBudget {
            used: 0,
            limit: crate::links::MAX_FETCH_BYTES,
        },
        cache: BTreeMap::new(),
        pending: BTreeMap::new(),
        work: 0,
    };
    let mut bytes = crate::links::FetchBudget {
        used: 0,
        limit: crate::links::MAX_FETCH_BYTES,
    };
    let mut metrics = crate::links::FetchMetrics::default();
    loop {
        walker.output_bytes.used = 0;
        let output = inputs
            .iter()
            .map(|(value, parts)| walker.walk(value, parts))
            .collect::<Result<Vec<_>>>()?;
        if walker.pending.is_empty() {
            let mut output_bytes = crate::links::FetchBudget {
                used: 0,
                limit: crate::links::MAX_FETCH_BYTES,
            };
            for value in &output {
                output_bytes.charge(value)?;
                budget.value(value)?;
            }
            return Ok((output, metrics));
        }
        let pending = std::mem::take(&mut walker.pending);
        let references = pending.values().cloned().collect::<Vec<_>>();
        let (values, counters) =
            connection.fetch_records_profiled_with_budget(&references, None)?;
        metrics.batches += counters.batches;
        metrics.rows_read += counters.rows_read;
        metrics.vm_steps += counters.vm_steps;
        for (key, value) in pending.into_keys().zip(values) {
            bytes.charge(&value)?;
            walker.cache.insert(key, value);
        }
    }
}

/// Rewrite only paths containing wildcard/index steps. A single `name.*` is
/// left to SQL name resolution so an existing table alias always wins.
pub(crate) fn expand(sql: &str) -> Result<String> {
    let mut tokens = Vec::new();
    for token in fastql_parser::tokenize(sql)? {
        // The shared SQL tokenizer reads `0.1.` as a number. Within a path,
        // those are two positions and separators. Original SQL bytes remain
        // untouched unless an entire extension path is recognized below.
        if token.kind == Kind::Number
            && token.text.contains('.')
            && token.text.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        {
            let extra = token.text.bytes().filter(|b| *b == b'.').count()
                + token
                    .text
                    .split('.')
                    .filter(|part| !part.is_empty())
                    .count();
            if extra > fastql_parser::MAX_TOKENS.saturating_sub(tokens.len()) {
                return Err(Error::Limit("projection token limit exceeded".into()));
            }
            let mut start = token.start;
            for part in token.text.split_inclusive('.') {
                let number = part.trim_end_matches('.');
                if !number.is_empty() {
                    tokens.push(Token {
                        kind: Kind::Number,
                        text: number.into(),
                        start,
                        end: start + number.len(),
                    });
                    start += number.len();
                }
                if part.ends_with('.') {
                    tokens.push(Token {
                        kind: Kind::Symbol,
                        text: ".".into(),
                        start,
                        end: start + 1,
                    });
                    start += 1;
                }
            }
        } else {
            if tokens.len() == fastql_parser::MAX_TOKENS {
                return Err(Error::Limit("projection token limit exceeded".into()));
            }
            tokens.push(token);
        }
    }
    let name = |t: &Token| matches!(t.kind, Kind::Word | Kind::Identifier);
    let mut out = String::new();
    let mut copied = 0;
    let mut i = 0;
    while i < tokens.len() {
        if !name(&tokens[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let mut end = i;
        let mut base_end = i;
        let mut special = false;
        let mut parts = Vec::new();
        let mut depth = 0;
        loop {
            // Adjacent bracket positions are FastQL paths; spaced bracket names
            // retain SQL alias/identifier parsing.
            if let Some(token) = tokens
                .get(end + 1)
                .filter(|t| t.start == tokens[end].end && sql.as_bytes()[t.start] == b'[')
            {
                let literal = token.text.trim();
                let index = if literal == "$" {
                    Some(-1)
                } else {
                    literal.parse::<i64>().ok()
                };
                if let Some(index) = index {
                    special = true;
                    parts.push(Part::Index(index));
                    end += 1;
                    depth += 1;
                    if depth > 64 {
                        return Err(Error::Limit("projection path nesting exceeds 64".into()));
                    }
                    continue;
                }
            }
            if tokens.get(end + 1).is_none_or(|t| t.text != ".") {
                break;
            }
            let Some(next) = tokens.get(end + 2) else {
                break;
            };
            let mut next_end = end + 2;
            let part = if name(next) {
                Part::Key(next.text.clone())
            } else if next.text == "*" {
                special = true;
                Part::All
            } else {
                let negative = next.text == "-";
                let number = if negative {
                    let Some(t) = tokens.get(end + 3) else { break };
                    next_end += 1;
                    t
                } else {
                    next
                };
                if number.kind != Kind::Number || !number.text.bytes().all(|b| b.is_ascii_digit()) {
                    break;
                }
                let literal = if negative {
                    format!("-{}", number.text)
                } else {
                    number.text.clone()
                };
                let index = literal.parse::<i64>().map_err(|_| {
                    Error::Validation("array position exceeds signed 64-bit range".into())
                })?;
                special = true;
                Part::Index(index)
            };
            end = next_end;
            if special {
                parts.push(part);
            } else {
                base_end = end;
            }
            depth += 1;
            if depth > 64 {
                return Err(Error::Limit("projection path nesting exceeds 64".into()));
            }
        }
        let single_star = base_end == start && matches!(parts.as_slice(), [Part::All]);
        if special && !single_star {
            let base = &sql[tokens[start].start..tokens[base_end].end];
            let encoded = serde_json::to_string(&parts)?.replace('\'', "''");
            out.push_str(&sql[copied..tokens[start].start]);
            out.push_str(&format!("__fastdb_h_doc_project({base},'{encoded}')"));
            copied = tokens[end].end;
        }
        i = end + 1;
    }
    out.push_str(&sql[copied..]);
    Ok(out)
}

/// Star resolution happens after parsing, so detect even spaced/commented
/// field stars before starting the snapshot shared by source and target reads.
pub(crate) fn has_wildcard(sql: &str) -> Result<bool> {
    Ok(fastql_parser::tokenize(sql)?
        .windows(2)
        .any(|tokens| tokens[0].text == "." && tokens[1].text == "*"))
}
