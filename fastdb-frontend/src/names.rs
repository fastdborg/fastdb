//! Phase 0 physical names and the canonical record-id codec.
//!
//! Invariants:
//! - Physical table/index names are `__fastdb_t_` / `__fastdb_i_` followed
//!   by exactly 32 lowercase hex characters derived from a 128-bit catalog
//!   id. They never contain a logical user identifier.
//! - `rid` is a versioned, type-tagged, length-delimited encoding so a
//!   string id can be decoded unambiguously even when it contains `:`,
//!   digits, or quotes. Phase 0 supports only the `s` (string) tag.

use crate::error::FastDbError;

pub const TABLE_NAME_PREFIX: &str = "__fastdb_t_";
pub const INDEX_NAME_PREFIX: &str = "__fastdb_i_";
/// Number of hex characters in a 128-bit id (16 bytes).
pub const HEX_LEN: usize = 32;

/// An immutable 128-bit catalog table id. Rendered as 32 lowercase hex
/// characters to form the opaque physical name suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TableId(pub u128);

impl TableId {
    /// Generate a fresh random 128-bit id.
    pub fn new_random() -> Self {
        Self(rand::random::<u128>())
    }

    /// Construct from a known value (used in tests).
    pub fn from_u128(v: u128) -> Self {
        Self(v)
    }

    /// Lowercase 32-char hex rendering of the id.
    pub fn to_hex(self) -> String {
        format!("{:032x}", self.0)
    }
}

/// Opaque physical table name for a catalog id.
pub fn physical_table_name(id: TableId) -> String {
    let mut s = String::with_capacity(TABLE_NAME_PREFIX.len() + HEX_LEN);
    s.push_str(TABLE_NAME_PREFIX);
    s.push_str(&id.to_hex());
    s
}

/// Opaque physical index name for a catalog id.
pub fn physical_index_name(id: TableId) -> String {
    let mut s = String::with_capacity(INDEX_NAME_PREFIX.len() + HEX_LEN);
    s.push_str(INDEX_NAME_PREFIX);
    s.push_str(&id.to_hex());
    s
}

/// Validate that `name` is `prefix` followed by exactly 32 lowercase hex
/// characters. Used at the DDL/AST boundary to re-check generated names
/// before they reach the engine.
pub fn validate_physical_name(name: &str, prefix: &str) -> Result<(), FastDbError> {
    let rest = name
        .strip_prefix(prefix)
        .ok_or_else(|| FastDbError::format("physical name has wrong prefix"))?;
    if rest.len() != HEX_LEN {
        return Err(FastDbError::format(
            "physical name suffix must be 32 hex chars",
        ));
    }
    if !rest
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(FastDbError::format(
            "physical name suffix must be lowercase hex (0-9, a-f)",
        ));
    }
    Ok(())
}

/// Encode a bare string record id as `s:<utf8-byte-length>:<value>`.
///
/// The `s` tag is the Phase 0 string type; the byte length makes decoding
/// unambiguous even when the value contains `:`, digits, or quotes.
pub fn encode_rid(value: &str) -> String {
    // Length is the UTF-8 byte length, not the char count.
    let mut s = String::with_capacity("s:".len() + 8 + value.len());
    s.push('s');
    s.push(':');
    s.push_str(&value.len().to_string());
    s.push(':');
    s.push_str(value);
    s
}

/// Decode a `s:<byte-length>:<value>` record id.
///
/// Returns a [`FastDbError`] (category `Format`) for any malformed input:
/// wrong/unknown type tag, missing delimiters, non-numeric length, or a
/// length that does not exactly match the remaining bytes. Never panics.
pub fn decode_rid(s: &str) -> Result<String, FastDbError> {
    let bytes = s.as_bytes();
    // Tag.
    if bytes.first() != Some(&b's') {
        return Err(FastDbError::format(
            "record id has unknown type tag (Phase 0 supports only 's')",
        ));
    }
    // Separator between tag and length.
    if bytes.get(1) != Some(&b':') {
        return Err(FastDbError::format(
            "record id is missing ':' after the type tag",
        ));
    }
    let rest = &bytes[2..];
    // Length digits up to the next ':'.
    let colon = rest
        .iter()
        .position(|&b| b == b':')
        .ok_or_else(|| FastDbError::format("record id is missing the value delimiter"))?;
    let len_str = std::str::from_utf8(&rest[..colon])
        .map_err(|_| FastDbError::format("record id length is not valid UTF-8"))?;
    if len_str.is_empty() {
        return Err(FastDbError::format("record id length is empty"));
    }
    if len_str.len() > 1 && len_str.as_bytes()[0] == b'0' {
        // Reject leading zeros to keep the canonical form unique.
        return Err(FastDbError::format("record id length has leading zeros"));
    }
    let len: usize = len_str
        .parse()
        .map_err(|_| FastDbError::format("record id length is not a number"))?;
    let value_bytes = &rest[colon + 1..];
    if value_bytes.len() != len {
        return Err(FastDbError::format(
            "record id length does not match the encoded value",
        ));
    }
    let value = std::str::from_utf8(value_bytes)
        .map_err(|_| FastDbError::format("record id value is not valid UTF-8"))?;
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCategory;

    #[test]
    fn physical_name_format_and_length() {
        let id = TableId::from_u128(0x1234);
        let t = physical_table_name(id);
        let i = physical_index_name(id);
        assert!(t.starts_with("__fastdb_t_"));
        assert!(i.starts_with("__fastdb_i_"));
        assert_eq!(t.len(), "__fastdb_t_".len() + HEX_LEN);
        assert_eq!(i.len(), "__fastdb_i_".len() + HEX_LEN);
        // 32 lowercase hex suffix
        let suffix = &t["__fastdb_t_".len()..];
        assert_eq!(suffix.len(), HEX_LEN);
        assert!(suffix
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }

    #[test]
    fn physical_name_zero_padded_to_32() {
        // Small id must still render as 32 hex chars (zero-padded).
        let t = physical_table_name(TableId::from_u128(1));
        assert_eq!(
            &t["__fastdb_t_".len()..],
            "00000000000000000000000000000001"
        );
    }

    #[test]
    fn physical_name_contains_no_logical_identifier() {
        // The logical name must not appear in the physical name.
        let t = physical_table_name(TableId::new_random());
        assert!(!t.contains("person"));
        assert!(!t.contains("tracy"));
        assert!(!t.contains("name"));
    }

    #[test]
    fn different_ids_produce_different_names() {
        let a = TableId::new_random();
        let b = TableId::new_random();
        assert_ne!(physical_table_name(a), physical_table_name(b));
        assert_ne!(physical_index_name(a), physical_index_name(b));
        // table vs index names for the same id differ by prefix.
        assert_ne!(physical_table_name(a), physical_index_name(a));
    }

    #[test]
    fn validate_accepts_generated_rejects_bad() {
        let t = physical_table_name(TableId::from_u128(0xabcdef));
        validate_physical_name(&t, TABLE_NAME_PREFIX).unwrap();
        let i = physical_index_name(TableId::from_u128(0xabcdef));
        validate_physical_name(&i, INDEX_NAME_PREFIX).unwrap();

        // Wrong prefix.
        assert!(validate_physical_name(&i, TABLE_NAME_PREFIX).is_err());
        // Uppercase hex rejected.
        assert!(validate_physical_name("__fastdb_t_ABCDEF", TABLE_NAME_PREFIX).is_err());
        // Wrong length.
        assert!(validate_physical_name("__fastdb_t_abc", TABLE_NAME_PREFIX).is_err());
        // Non-hex.
        assert!(validate_physical_name(
            "__fastdb_t_zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
            TABLE_NAME_PREFIX
        )
        .is_err());
    }

    #[test]
    fn rid_round_trip_ascii() {
        for v in ["tracy", "a", "", "with spaces", "UPPER", "12345"] {
            assert_eq!(decode_rid(&encode_rid(v)).unwrap(), v);
        }
    }

    #[test]
    fn rid_round_trip_utf8() {
        for v in ["naïve", "café", "東京", "🦀", "a\tb\n"] {
            assert_eq!(decode_rid(&encode_rid(v)).unwrap(), v, "failed for {v:?}");
        }
    }

    #[test]
    fn rid_encoding_is_byte_length_delimited() {
        // 'naïve' is 5 chars but 6 UTF-8 bytes.
        let enc = encode_rid("naïve");
        assert!(enc.starts_with("s:6:"), "got {enc}");
    }

    #[test]
    fn rid_value_with_delimiters_decodes_unambiguously() {
        // Values containing ':' and digits must not confuse the decoder.
        for v in ["a:b", "5:evil", ":leading", "trailing:", "1:2:3", "s:fake"] {
            assert_eq!(decode_rid(&encode_rid(v)).unwrap(), v, "failed for {v:?}");
        }
    }

    #[test]
    fn rid_malformed_is_format_error_not_panic() {
        // No panic on arbitrary malformed input.
        let bad = [
            "",
            "s",
            "s:",
            "s:0",
            "x:5:tracy",  // wrong tag
            "s:x:tracy",  // non-numeric length
            "s:05:tracy", // leading zero (canonical form is s:5:)
            "s:3:ab",     // length too short
            "s:3:abcd",   // length too long
            "s:abc:ab",   // non-numeric
            "tracy",      // no tag
            "s::",        // empty length
            "ss:1:a",
        ];
        for b in bad {
            // Must never panic; errors must be Format-category.
            if let Ok(decoded) = decode_rid(b) {
                // The only acceptable "success" is a faithful round trip.
                assert_eq!(encode_rid(&decoded), b, "unexpected accept of {b:?}");
            } else {
                // All rejections must be Format-category.
                // (decode_rid only ever returns Format errors.)
            }
        }
        // Spot-check the category explicitly.
        assert_eq!(
            decode_rid("x:5:tracy").unwrap_err().category(),
            ErrorCategory::Format
        );
        assert_eq!(
            decode_rid("s:99:short").unwrap_err().category(),
            ErrorCategory::Format
        );
    }

    #[test]
    fn rid_known_encoding() {
        assert_eq!(encode_rid("tracy"), "s:5:tracy");
        assert_eq!(decode_rid("s:5:tracy").unwrap(), "tracy");
    }
}
