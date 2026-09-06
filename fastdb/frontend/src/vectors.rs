//! Validated vector representations from the pinned engine.
use crate::{Error, Result, Value};
fn invalid() -> Error {
    Error::Validation("invalid vector representation".into())
}
fn check_dims(dims: usize) -> Result<usize> {
    if dims == 0 {
        return Err(invalid());
    }
    if dims > 65_536 {
        return Err(Error::Limit("vector dimensions exceed 65536".into()));
    }
    Ok(dims)
}
fn finite32(bytes: &[u8]) -> Result<()> {
    for chunk in bytes.chunks_exact(4) {
        if !f32::from_le_bytes(chunk.try_into().expect("f32 width")).is_finite() {
            return Err(Error::Validation("vector components must be finite".into()));
        }
    }
    Ok(())
}
pub(crate) fn dimensions(bytes: &[u8]) -> Result<usize> {
    let (data, kind) = if bytes.len() % 2 == 0 {
        (bytes, 1)
    } else {
        (
            &bytes[..bytes.len() - 1],
            *bytes.last().expect("odd length"),
        )
    };
    match kind {
        1 | 2 => {
            let width = if kind == 1 { 4 } else { 8 };
            if data.len() % width != 0 {
                return Err(invalid());
            }
            let dims = check_dims(data.len() / width)?;
            if kind == 1 {
                finite32(data)?;
            } else {
                for chunk in data.chunks_exact(8) {
                    if !f64::from_le_bytes(chunk.try_into().expect("f64 width")).is_finite() {
                        return Err(Error::Validation("vector components must be finite".into()));
                    }
                }
            }
            Ok(dims)
        }
        9 => {
            if data.len() < 4 || (data.len() - 4) % 8 != 0 {
                return Err(invalid());
            }
            let end = data.len() - 4;
            let dims = check_dims(u32::from_le_bytes(
                data[end..].try_into().expect("dimension width"),
            ) as usize)?;
            let count = end / 8;
            if count > dims {
                return Err(invalid());
            }
            finite32(&data[..count * 4])?;
            let mut previous = None;
            for index in data[count * 4..end].chunks_exact(4) {
                let index = u32::from_le_bytes(index.try_into().expect("index width")) as usize;
                if index >= dims || previous.is_some_and(|p| p >= index) {
                    return Err(invalid());
                }
                previous = Some(index);
            }
            Ok(dims)
        }
        3 => {
            if data.len() < 2 {
                return Err(invalid());
            }
            let trailing = *data.last().expect("metadata") as usize;
            let dims = check_dims(
                data.len()
                    .checked_mul(8)
                    .and_then(|n| n.checked_sub(trailing))
                    .ok_or_else(invalid)?,
            )?;
            let size = dims.div_ceil(8);
            let padding = usize::from(size % 2 == 0);
            if size + padding + 1 != data.len()
                || data[size..size + padding].iter().any(|b| *b != 0)
            {
                return Err(invalid());
            }
            if dims % 8 != 0 && data[size - 1] >> (dims % 8) != 0 {
                return Err(invalid());
            }
            Ok(dims)
        }
        4 => {
            if data.len() < 14 {
                return Err(invalid());
            }
            let aligned = data.len() - 10;
            let trailing = data[data.len() - 1] as usize;
            if aligned % 4 != 0 || trailing > 3 || data[data.len() - 2] != 0 {
                return Err(invalid());
            }
            let dims = check_dims(aligned.checked_sub(trailing).ok_or_else(invalid)?)?;
            if data[dims..aligned].iter().any(|b| *b != 0) {
                return Err(invalid());
            }
            let alpha =
                f32::from_le_bytes(data[aligned..aligned + 4].try_into().expect("scale width"));
            let shift = f32::from_le_bytes(
                data[aligned + 4..aligned + 8]
                    .try_into()
                    .expect("shift width"),
            );
            if !alpha.is_finite()
                || !shift.is_finite()
                || data[..dims]
                    .iter()
                    .any(|q| !(alpha * *q as f32 + shift).is_finite())
            {
                return Err(Error::Validation("vector components must be finite".into()));
            }
            Ok(dims)
        }
        _ => Err(Error::Unsupported("unknown vector representation".into())),
    }
}
pub(crate) fn validate_dimension(dims: usize) -> Result<()> {
    if !(1..=65_536).contains(&dims) {
        return Err(Error::Validation(
            "vector field dimensions must be 1..65536".into(),
        ));
    }
    Ok(())
}
impl Value {
    pub fn vector32(values: &[f32]) -> Result<Self> {
        let value = Self::Vector(values.iter().flat_map(|v| v.to_le_bytes()).collect());
        value.validate()?;
        Ok(value)
    }
    pub fn vector64(values: &[f64]) -> Result<Self> {
        let mut bytes = values
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        bytes.push(2);
        let value = Self::Vector(bytes);
        value.validate()?;
        Ok(value)
    }
    pub fn vector_dimensions(&self) -> Result<usize> {
        match self {
            Self::Vector(bytes) => dimensions(bytes),
            _ => Err(Error::Validation("expected typed vector".into())),
        }
    }
}

/// The pinned native sparse concat does not shift the right-hand indexes.
/// Correct that representation in the typed frontend; other formats use the
/// upstream operation unchanged.
pub(crate) fn concat(
    left: crate::EngineValue,
    right: crate::EngineValue,
) -> Result<crate::EngineValue> {
    use turso_core::vector::{
        operations::{concat::vector_concat, serialize::vector_serialize},
        parse_vector,
        vector_types::VectorType,
    };
    let a = parse_vector(&left, None)?;
    let b = parse_vector(&right, None)?;
    let crate::EngineValue::Blob(a_bytes) = vector_serialize(parse_vector(&left, None)?) else {
        unreachable!("vector blob");
    };
    let crate::EngineValue::Blob(b_bytes) = vector_serialize(parse_vector(&right, None)?) else {
        unreachable!("vector blob");
    };
    dimensions(&a_bytes)?;
    dimensions(&b_bytes)?;
    check_dims(a.dims.checked_add(b.dims).ok_or_else(invalid)?)?;
    if a.vector_type == VectorType::Float32Sparse && b.vector_type == VectorType::Float32Sparse {
        let ac = (a_bytes.len() - 5) / 8;
        let bc = (b_bytes.len() - 5) / 8;
        let mut bytes = a_bytes[..ac * 4].to_vec();
        bytes.extend_from_slice(&b_bytes[..bc * 4]);
        bytes.extend_from_slice(&a_bytes[ac * 4..ac * 8]);
        for idx in b_bytes[bc * 4..bc * 8].chunks_exact(4) {
            let index = u32::from_le_bytes(idx.try_into().expect("index width"));
            bytes.extend((index + a.dims as u32).to_le_bytes());
        }
        bytes.extend(((a.dims + b.dims) as u32).to_le_bytes());
        bytes.push(9);
        dimensions(&bytes)?;
        return Ok(crate::EngineValue::Blob(bytes));
    }
    Ok(vector_serialize(vector_concat(&a, &b)?))
}
