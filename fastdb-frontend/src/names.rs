//! Opaque physical-name and stable format-1 record-ID codecs.

use crate::decode::RecordIdValue;
use crate::error::FastDbError;

pub const TABLE_NAME_PREFIX: &str = "__fastdb_t_";
pub const INDEX_NAME_PREFIX: &str = "__fastdb_i_";
pub const HEX_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogId(pub u128);

pub type TableId = CatalogId;
pub type IndexId = CatalogId;

impl CatalogId {
    pub fn new_random() -> Self {
        Self(rand::random::<u128>())
    }

    pub const fn from_u128(value: u128) -> Self {
        Self(value)
    }

    pub fn from_hex(value: &str) -> Result<Self, FastDbError> {
        if value.len() != HEX_LEN
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(FastDbError::format(
                "catalog ID must be exactly 32 lowercase hexadecimal characters",
            ));
        }
        u128::from_str_radix(value, 16)
            .map(Self)
            .map_err(|_| FastDbError::format("catalog ID is outside the 128-bit range"))
    }

    pub fn to_hex(self) -> String {
        format!("{:032x}", self.0)
    }
}

pub fn physical_table_name(id: TableId) -> String {
    format!("{TABLE_NAME_PREFIX}{}", id.to_hex())
}

pub fn physical_index_name(id: IndexId) -> String {
    format!("{INDEX_NAME_PREFIX}{}", id.to_hex())
}

pub fn validate_physical_name(name: &str, prefix: &str) -> Result<(), FastDbError> {
    let rest = name
        .strip_prefix(prefix)
        .ok_or_else(|| FastDbError::format("physical name has wrong prefix"))?;
    CatalogId::from_hex(rest).map(|_| ())
}

pub fn encode_rid(value: impl Into<RecordIdValue>) -> String {
    match value.into() {
        RecordIdValue::String(value) => format!("v1:s:{}:{value}", value.len()),
        RecordIdValue::Integer(value) => format!("v1:i:{value}"),
        RecordIdValue::Uuid(value) => format!("v1:u:{}", value.hyphenated()),
    }
}

pub fn decode_rid(encoded: &str) -> Result<RecordIdValue, FastDbError> {
    let payload = encoded
        .strip_prefix("v1:")
        .ok_or_else(|| FastDbError::format("record ID has an unknown encoding version"))?;
    if let Some(rest) = payload.strip_prefix("s:") {
        return decode_string_rid(rest).map(RecordIdValue::String);
    }
    if let Some(value) = payload.strip_prefix("i:") {
        let parsed = value
            .parse::<i64>()
            .map_err(|_| FastDbError::format("record ID integer is outside the i64 range"))?;
        if parsed.to_string() != value {
            return Err(FastDbError::format(
                "record ID integer is not canonically encoded",
            ));
        }
        return Ok(RecordIdValue::Integer(parsed));
    }
    if let Some(value) = payload.strip_prefix("u:") {
        let parsed = uuid::Uuid::parse_str(value)
            .map_err(|_| FastDbError::format("record ID UUID is malformed"))?;
        if parsed.hyphenated().to_string() != value || !matches!(parsed.get_version_num(), 4 | 7) {
            return Err(FastDbError::format(
                "record ID UUID is not a canonical UUIDv4 or UUIDv7",
            ));
        }
        return Ok(RecordIdValue::Uuid(parsed));
    }
    Err(FastDbError::format("record ID has an unknown type tag"))
}

fn decode_string_rid(value: &str) -> Result<String, FastDbError> {
    let (length, value) = value
        .split_once(':')
        .ok_or_else(|| FastDbError::format("record ID string has no value delimiter"))?;
    if length.is_empty()
        || !length.bytes().all(|byte| byte.is_ascii_digit())
        || (length.len() > 1 && length.starts_with('0'))
    {
        return Err(FastDbError::format(
            "record ID string length is not canonical",
        ));
    }
    let expected = length
        .parse::<usize>()
        .map_err(|_| FastDbError::format("record ID string length is outside usize"))?;
    if value.len() != expected {
        return Err(FastDbError::format(
            "record ID string length does not match its UTF-8 bytes",
        ));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCategory;

    #[test]
    fn p2_codec_003_physical_names_are_opaque_and_validated() {
        let id = CatalogId::from_u128(0x1234);
        let table = physical_table_name(id);
        let index = physical_index_name(id);
        assert_eq!(table, "__fastdb_t_00000000000000000000000000001234");
        assert_eq!(index, "__fastdb_i_00000000000000000000000000001234");
        validate_physical_name(&table, TABLE_NAME_PREFIX).unwrap();
        validate_physical_name(&index, INDEX_NAME_PREFIX).unwrap();
        assert!(validate_physical_name("__fastdb_t_AB", TABLE_NAME_PREFIX).is_err());
    }

    #[test]
    fn p2_codec_004_rid_types_boundaries_unicode_and_non_collision() {
        let values = [
            RecordIdValue::String("tracy".into()),
            RecordIdValue::String("東京:🦀".into()),
            RecordIdValue::Integer(i64::MIN),
            RecordIdValue::Integer(i64::MAX),
            RecordIdValue::Uuid(
                uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            ),
        ];
        for value in values {
            assert_eq!(decode_rid(&encode_rid(&value)).unwrap(), value);
        }
        assert_ne!(
            encode_rid(RecordIdValue::String("1".into())),
            encode_rid(RecordIdValue::Integer(1))
        );
    }

    #[test]
    fn p2_codec_005_malformed_rids_are_format_errors() {
        for value in [
            "",
            "s:5:tracy",
            "v2:s:1:a",
            "v1:x:a",
            "v1:s:01:a",
            "v1:s:2:a",
            "v1:i:+1",
            "v1:i:01",
            "v1:i:-0",
            "v1:u:6ba7b810-9dad-11d1-80b4-00c04fd430c8",
        ] {
            assert_eq!(
                decode_rid(value).unwrap_err().category(),
                ErrorCategory::Format
            );
        }
    }
}
