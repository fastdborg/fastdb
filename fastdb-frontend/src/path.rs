//! Canonical JSON path construction and in-memory document traversal.

use crate::decode::Value;
use crate::error::{FastDbError, Result};
use std::collections::BTreeMap;

/// Encode path segments as JSON-escaped dot-quoted components.
pub fn canonical_path<I, S>(segments: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut output = String::from("$");
    let mut count = 0usize;
    for segment in segments {
        let segment = segment.as_ref();
        if segment.is_empty() {
            return Err(FastDbError::Schema(
                "field path contains an empty segment".into(),
            ));
        }
        let encoded = serde_json::to_string(segment)
            .map_err(|error| FastDbError::Engine(format!("failed to encode JSON path: {error}")))?;
        output.push('.');
        output.push_str(&encoded);
        count += 1;
    }
    if count == 0 {
        return Err(FastDbError::Schema("field path is empty".into()));
    }
    Ok(output)
}

pub fn parser_path(path: &turso_fastdb_parser::FieldPath) -> Result<(Vec<String>, String)> {
    let segments = path
        .segments
        .iter()
        .map(|segment| segment.value.clone())
        .collect::<Vec<_>>();
    let key = canonical_path(&segments)?;
    Ok((segments, key))
}

pub fn decode_canonical_path(path: &str) -> Result<Vec<String>> {
    let mut position = 0usize;
    if !path.starts_with('$') {
        return Err(FastDbError::format(
            "canonical field path does not start with `$`",
        ));
    }
    position += 1;
    let mut segments = Vec::new();
    while position < path.len() {
        if path.as_bytes().get(position) != Some(&b'.') {
            return Err(FastDbError::format(
                "canonical field path has a malformed segment separator",
            ));
        }
        position += 1;
        let mut stream =
            serde_json::Deserializer::from_str(&path[position..]).into_iter::<String>();
        let segment = stream
            .next()
            .transpose()
            .map_err(|_| FastDbError::format("canonical field path has invalid JSON escaping"))?
            .ok_or_else(|| FastDbError::format("canonical field path has an empty tail"))?;
        let consumed = stream.byte_offset();
        if consumed == 0 || segment.is_empty() {
            return Err(FastDbError::format(
                "canonical field path contains an empty segment",
            ));
        }
        position += consumed;
        segments.push(segment);
    }
    if segments.is_empty() || canonical_path(&segments)? != path {
        return Err(FastDbError::format("field path is not canonically encoded"));
    }
    Ok(segments)
}

pub fn get_path<'a>(document: &'a BTreeMap<String, Value>, path: &[String]) -> Option<&'a Value> {
    let (first, rest) = path.split_first()?;
    let mut value = document.get(first)?;
    for segment in rest {
        let Value::Object(object) = value else {
            return None;
        };
        value = object.get(segment)?;
    }
    Some(value)
}

pub fn get_path_mut<'a>(
    document: &'a mut BTreeMap<String, Value>,
    path: &[String],
) -> Option<&'a mut Value> {
    let (first, rest) = path.split_first()?;
    let mut value = document.get_mut(first)?;
    for segment in rest {
        let Value::Object(object) = value else {
            return None;
        };
        value = object.get_mut(segment)?;
    }
    Some(value)
}

/// Build the nested object structure for one SET assignment.
pub fn set_path(
    document: &mut BTreeMap<String, Value>,
    path: &[String],
    value: Value,
) -> Result<()> {
    let (last, parents) = path
        .split_last()
        .ok_or_else(|| FastDbError::Schema("field path is empty".into()))?;
    let mut object = document;
    for segment in parents {
        let entry = object
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(BTreeMap::new()));
        if !matches!(entry, Value::Object(_)) {
            *entry = Value::Object(BTreeMap::new());
        }
        let Value::Object(next) = entry else {
            unreachable!("entry was normalized to an object")
        };
        object = next;
    }
    object.insert(last.clone(), value);
    Ok(())
}

/// Remove a path while retaining now-empty ancestor objects.
pub fn remove_path(document: &mut BTreeMap<String, Value>, path: &[String]) -> Result<()> {
    let (last, parents) = path
        .split_last()
        .ok_or_else(|| FastDbError::Schema("field path is empty".into()))?;
    let mut object = document;
    for segment in parents {
        let Some(Value::Object(next)) = object.get_mut(segment) else {
            return Ok(());
        };
        object = next;
    }
    object.remove(last);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p2_path_001_quotes_every_adversarial_segment() {
        let path = canonical_path([
            "profile",
            "a.b",
            "quote\"",
            "br[ack]et",
            "back\\slash",
            "line\nfeed",
            "東京",
        ])
        .unwrap();
        assert_eq!(
            path,
            r#"$."profile"."a.b"."quote\""."br[ack]et"."back\\slash"."line\nfeed"."東京""#
        );
    }

    #[test]
    fn p2_path_002_nested_set_and_get_share_segments() {
        let path = vec!["profile".into(), "age".into()];
        let mut document = BTreeMap::new();
        set_path(&mut document, &path, Value::Integer(42)).unwrap();
        assert_eq!(get_path(&document, &path), Some(&Value::Integer(42)));
        let encoded = canonical_path(&path).unwrap();
        assert_eq!(decode_canonical_path(&encoded).unwrap(), path);
    }
}
