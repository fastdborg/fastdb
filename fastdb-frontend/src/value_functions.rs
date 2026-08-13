use crate::error::{FastDbError, Result};
use crate::Value;
use diff_match_patch_rs::{Compat, DiffMatchPatch, PatchInput};
use std::collections::BTreeMap;

const MAX_PATCH_OPERATIONS: usize = 65_536;
const MAX_PATCH_TEXT_BYTES: usize = 1 << 20;
const MAX_PATCH_DEPTH: usize = 64;

pub(crate) fn diff(left: &Value, right: &Value) -> Result<Value> {
    let mut operations = Vec::new();
    diff_at(left, right, "", 0, &mut operations)?;
    Ok(Value::Array(operations))
}

pub(crate) fn patch(value: &Value, patch: &Value) -> Result<Value> {
    let Value::Array(operations) = patch else {
        return Err(FastDbError::Schema(
            "value::patch requires an array of patch operations".into(),
        ));
    };
    if operations.len() > MAX_PATCH_OPERATIONS {
        return Err(FastDbError::ResourceLimit(format!(
            "value patch exceeds {MAX_PATCH_OPERATIONS} operations"
        )));
    }

    // Work on a clone so a failed operation never exposes a partially patched
    // value to the containing statement.
    let mut output = value.clone();
    for operation in operations {
        apply_operation(&mut output, operation)?;
    }
    Ok(output)
}

fn diff_at(
    left: &Value,
    right: &Value,
    path: &str,
    depth: usize,
    operations: &mut Vec<Value>,
) -> Result<()> {
    check_depth(depth)?;
    if left == right {
        return Ok(());
    }
    if operations.len() >= MAX_PATCH_OPERATIONS {
        return Err(FastDbError::ResourceLimit(format!(
            "value diff exceeds {MAX_PATCH_OPERATIONS} operations"
        )));
    }

    match (left, right) {
        (Value::Str(left), Value::Str(right)) => {
            check_text(left)?;
            check_text(right)?;
            let dmp = DiffMatchPatch::new();
            let diffs = dmp
                .diff_main::<Compat>(left, right)
                .map_err(|error| FastDbError::Schema(format!("string diff failed: {error:?}")))?;
            let patches = dmp
                .patch_make(PatchInput::new_diffs(&diffs))
                .map_err(|error| FastDbError::Schema(format!("string diff failed: {error:?}")))?;
            let patch_text = dmp.patch_to_text(&patches);
            check_text(&patch_text)?;
            operations.push(operation(
                "change",
                if path.is_empty() { "/" } else { path },
                Some(Value::Str(patch_text)),
                None,
            ));
        }
        (Value::Object(left), Value::Object(right)) => {
            for key in left.keys().filter(|key| !right.contains_key(*key)) {
                operations.push(operation("remove", &child_path(path, key), None, None));
            }
            for (key, left_value) in left {
                if let Some(right_value) = right.get(key) {
                    diff_at(
                        left_value,
                        right_value,
                        &child_path(path, key),
                        depth + 1,
                        operations,
                    )?;
                }
            }
            for (key, right_value) in right.iter().filter(|(key, _)| !left.contains_key(*key)) {
                operations.push(operation(
                    "add",
                    &child_path(path, key),
                    Some(right_value.clone()),
                    None,
                ));
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            for index in 0..left.len().min(right.len()) {
                diff_at(
                    &left[index],
                    &right[index],
                    &child_path(path, &index.to_string()),
                    depth + 1,
                    operations,
                )?;
            }
            for index in (right.len()..left.len()).rev() {
                operations.push(operation(
                    "remove",
                    &child_path(path, &index.to_string()),
                    None,
                    None,
                ));
            }
            for (index, right_value) in right.iter().enumerate().skip(left.len()) {
                operations.push(operation(
                    "add",
                    &child_path(path, &index.to_string()),
                    Some(right_value.clone()),
                    None,
                ));
            }
        }
        _ => operations.push(operation("replace", path, Some(right.clone()), None)),
    }
    if operations.len() > MAX_PATCH_OPERATIONS {
        return Err(FastDbError::ResourceLimit(format!(
            "value diff exceeds {MAX_PATCH_OPERATIONS} operations"
        )));
    }
    Ok(())
}

fn apply_operation(target: &mut Value, operation: &Value) -> Result<()> {
    let Value::Object(fields) = operation else {
        return Err(FastDbError::Schema(
            "patch operation must be an object".into(),
        ));
    };
    let name = required_string(fields, "op")?;
    let path = required_string(fields, "path")?;
    if path.len() > MAX_PATCH_TEXT_BYTES {
        return Err(FastDbError::ResourceLimit(
            "patch path exceeds the byte limit".into(),
        ));
    }
    let components = path_components(path)?;
    check_depth(components.len())?;

    match name {
        "add" => add_at(target, &components, required_value(fields)?.clone()),
        "remove" => remove_at(target, &components).map(|_| ()),
        "replace" => replace_at(target, &components, required_value(fields)?.clone()),
        "test" => {
            let actual = get_at(target, &components)?;
            if actual == required_value(fields)? {
                Ok(())
            } else {
                Err(FastDbError::Constraint(
                    "patch test operation failed".into(),
                ))
            }
        }
        "copy" => {
            let from = path_components(required_string(fields, "from")?)?;
            let value = get_at(target, &from)?.clone();
            add_at(target, &components, value)
        }
        "move" => {
            let from = path_components(required_string(fields, "from")?)?;
            let value = remove_at(target, &from)?;
            add_at(target, &components, value)
        }
        "change" => change_at(target, path, &components, required_value(fields)?),
        _ => Err(FastDbError::Schema(format!(
            "unsupported patch operation {name:?}"
        ))),
    }
}

fn operation(name: &str, path: &str, value: Option<Value>, from: Option<&str>) -> Value {
    let mut fields = BTreeMap::from([
        ("op".into(), Value::Str(name.into())),
        ("path".into(), Value::Str(path.into())),
    ]);
    if let Some(value) = value {
        fields.insert("value".into(), value);
    }
    if let Some(from) = from {
        fields.insert("from".into(), Value::Str(from.into()));
    }
    Value::Object(fields)
}

fn child_path(parent: &str, component: &str) -> String {
    format!("{parent}/{component}")
}

fn path_components(path: &str) -> Result<Vec<&str>> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    let Some(path) = path.strip_prefix('/') else {
        return Err(FastDbError::Schema(
            "patch path must be empty or begin with `/`".into(),
        ));
    };
    Ok(path.split('/').collect())
}

fn required_string<'a>(fields: &'a BTreeMap<String, Value>, field: &str) -> Result<&'a str> {
    match fields.get(field) {
        Some(Value::Str(value)) => Ok(value),
        _ => Err(FastDbError::Schema(format!(
            "patch operation requires string field {field:?}"
        ))),
    }
}

fn required_value(fields: &BTreeMap<String, Value>) -> Result<&Value> {
    fields
        .get("value")
        .ok_or_else(|| FastDbError::Schema("patch operation requires field \"value\"".into()))
}

fn get_at<'a>(target: &'a Value, path: &[&str]) -> Result<&'a Value> {
    let Some((component, rest)) = path.split_first() else {
        return Ok(target);
    };
    match target {
        Value::Object(fields) => fields
            .get(*component)
            .ok_or_else(|| FastDbError::Schema("patch path does not exist".into()))
            .and_then(|value| get_at(value, rest)),
        Value::Array(values) => {
            let index = array_index(component, values.len(), false)?;
            get_at(&values[index], rest)
        }
        _ => Err(FastDbError::Schema(
            "patch path traverses a scalar value".into(),
        )),
    }
}

fn parent_mut<'a, 'b>(
    target: &'a mut Value,
    path: &'b [&'b str],
) -> Result<(&'a mut Value, &'b str)> {
    let Some((last, parent_path)) = path.split_last() else {
        return Err(FastDbError::Engine("root patch path has no parent".into()));
    };
    let mut parent = target;
    for component in parent_path {
        parent = match parent {
            Value::Object(fields) => fields
                .get_mut(*component)
                .ok_or_else(|| FastDbError::Schema("patch path does not exist".into()))?,
            Value::Array(values) => {
                let index = array_index(component, values.len(), false)?;
                &mut values[index]
            }
            _ => {
                return Err(FastDbError::Schema(
                    "patch path traverses a scalar value".into(),
                ));
            }
        };
    }
    Ok((parent, last))
}

fn add_at(target: &mut Value, path: &[&str], value: Value) -> Result<()> {
    if path.is_empty() {
        *target = value;
        return Ok(());
    }
    let (parent, component) = parent_mut(target, path)?;
    match parent {
        Value::Object(fields) => {
            fields.insert(component.to_string(), value);
            Ok(())
        }
        Value::Array(values) => {
            let index = array_index(component, values.len(), true)?;
            values.insert(index, value);
            Ok(())
        }
        _ => Err(FastDbError::Schema(
            "patch add target is not a collection".into(),
        )),
    }
}

fn remove_at(target: &mut Value, path: &[&str]) -> Result<Value> {
    if path.is_empty() {
        return Ok(std::mem::replace(target, Value::None));
    }
    let (parent, component) = parent_mut(target, path)?;
    match parent {
        Value::Object(fields) => fields
            .remove(component)
            .ok_or_else(|| FastDbError::Schema("patch remove path does not exist".into())),
        Value::Array(values) => {
            let index = array_index(component, values.len(), false)?;
            Ok(values.remove(index))
        }
        _ => Err(FastDbError::Schema(
            "patch remove target is not a collection".into(),
        )),
    }
}

fn replace_at(target: &mut Value, path: &[&str], value: Value) -> Result<()> {
    if path.is_empty() {
        *target = value;
        return Ok(());
    }
    let (parent, component) = parent_mut(target, path)?;
    match parent {
        Value::Object(fields) => {
            let target = fields
                .get_mut(component)
                .ok_or_else(|| FastDbError::Schema("patch replace path does not exist".into()))?;
            *target = value;
            Ok(())
        }
        Value::Array(values) => {
            let index = array_index(component, values.len(), false)?;
            values[index] = value;
            Ok(())
        }
        _ => Err(FastDbError::Schema(
            "patch replace target is not a collection".into(),
        )),
    }
}

fn change_at(target: &mut Value, raw_path: &str, path: &[&str], patch: &Value) -> Result<()> {
    let Value::Str(patch) = patch else {
        return Err(FastDbError::Schema(
            "patch change operation requires a string value".into(),
        ));
    };
    check_text(patch)?;
    let target = if raw_path == "/" || path.is_empty() {
        target
    } else {
        let (parent, component) = parent_mut(target, path)?;
        match parent {
            Value::Object(fields) => fields
                .get_mut(component)
                .ok_or_else(|| FastDbError::Schema("patch change path does not exist".into()))?,
            Value::Array(values) => {
                let index = array_index(component, values.len(), false)?;
                &mut values[index]
            }
            _ => {
                return Err(FastDbError::Schema(
                    "patch change target is not a collection".into(),
                ));
            }
        }
    };
    let Value::Str(source) = target else {
        return Err(FastDbError::Schema(
            "patch change target must be a string".into(),
        ));
    };
    check_text(source)?;
    let dmp = DiffMatchPatch::new();
    let patches = dmp
        .patch_from_text::<Compat>(patch)
        .map_err(|error| FastDbError::Schema(format!("invalid string patch: {error:?}")))?;
    let (changed, applied) = dmp
        .patch_apply(&patches, source)
        .map_err(|error| FastDbError::Schema(format!("string patch failed: {error:?}")))?;
    if applied.iter().any(|applied| !applied) {
        return Err(FastDbError::Constraint(
            "string patch does not apply to the target".into(),
        ));
    }
    check_text(&changed)?;
    *source = changed;
    Ok(())
}

fn array_index(component: &str, len: usize, allow_end: bool) -> Result<usize> {
    if allow_end && component == "-" {
        return Ok(len);
    }
    let index = component
        .parse::<usize>()
        .map_err(|_| FastDbError::Schema("patch array path is not an index".into()))?;
    if index < len || (allow_end && index == len) {
        Ok(index)
    } else {
        Err(FastDbError::Schema(
            "patch array index is out of bounds".into(),
        ))
    }
}

fn check_text(value: &str) -> Result<()> {
    if value.len() > MAX_PATCH_TEXT_BYTES {
        Err(FastDbError::ResourceLimit(format!(
            "value diff text exceeds {MAX_PATCH_TEXT_BYTES} bytes"
        )))
    } else {
        Ok(())
    }
}

fn check_depth(depth: usize) -> Result<()> {
    if depth > MAX_PATCH_DEPTH {
        Err(FastDbError::ResourceLimit(format!(
            "value patch exceeds nesting depth {MAX_PATCH_DEPTH}"
        )))
    } else {
        Ok(())
    }
}
