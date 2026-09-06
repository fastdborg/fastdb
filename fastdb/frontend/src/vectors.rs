//! Initial validated dense vector representations from the pinned engine.
use crate::{Error, Result, Value};
pub(crate) fn dimensions(bytes: &[u8]) -> Result<usize> {
    let invalid = || Error::Validation("invalid dense vector representation".into());
    let (data, width) = if bytes.len() % 2 == 0 {
        (bytes, 4)
    } else {
        match bytes.last() {
            Some(1) => (&bytes[..bytes.len() - 1], 4),
            Some(2) => (&bytes[..bytes.len() - 1], 8),
            _ => {
                return Err(Error::Unsupported(
                    "this vector representation is not validated yet".into(),
                ))
            }
        }
    };
    if data.is_empty() || data.len() % width != 0 {
        return Err(invalid());
    }
    let dims = data.len() / width;
    if dims > 65_536 {
        return Err(Error::Limit("vector dimensions exceed 65536".into()));
    }
    for chunk in data.chunks_exact(width) {
        let finite = if width == 4 {
            f32::from_le_bytes(chunk.try_into().expect("f32 width")).is_finite()
        } else {
            f64::from_le_bytes(chunk.try_into().expect("f64 width")).is_finite()
        };
        if !finite {
            return Err(Error::Validation("vector components must be finite".into()));
        }
    }
    Ok(dims)
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
