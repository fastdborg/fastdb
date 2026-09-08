//! JSON framing has more nesting than logical values: each tagged container
//! adds an object plus its payload, with additional transfer/parameter wrappers.
use crate::{Error, Result};

const MAX_WIRE_DEPTH: usize = 2 * 64 + 8;

pub(crate) fn deserializer(
    input: &str,
) -> Result<serde_json::Deserializer<serde_json::de::StrRead<'_>>> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for byte in input.bytes() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > MAX_WIRE_DEPTH {
                        return Err(Error::Limit("wire JSON nesting exceeds 136".into()));
                    }
                }
                b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    // Syntax (including mismatched closers and unterminated strings) remains
    // serde's responsibility. Disable its smaller default only after preflight.
    let mut decoder = serde_json::Deserializer::from_str(input);
    decoder.disable_recursion_limit();
    Ok(decoder)
}

/// Internal cross-crate binding support; logical values are validated separately.
#[doc(hidden)]
pub fn decode_wire_json<T: serde::de::DeserializeOwned>(input: &str) -> Result<T> {
    let mut decoder = deserializer(input)?;
    crate::parser_stack(|| {
        let result = T::deserialize(&mut decoder)?;
        decoder.end()?;
        Ok(result)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_preflight_counts_containers_but_ignores_escaped_string_content() {
        let text = format!("\"{}\"", "[{}]\\\"\\\\".repeat(200));
        let value: serde_json::Value = decode_wire_json(&text).unwrap();
        assert!(value.is_string());
        for depth in [128, MAX_WIRE_DEPTH] {
            let input = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
            let _: serde_json::Value = decode_wire_json(&input).unwrap();
        }
        let excessive = "[".repeat(MAX_WIRE_DEPTH + 1);
        assert_eq!(
            decode_wire_json::<serde_json::Value>(&excessive)
                .unwrap_err()
                .code(),
            "FDB_LIMIT"
        );
        for malformed in [
            "[}",
            "}[[",
            "\"unterminated",
            "{} trailing",
            "[1,]",
            "{\"a\":}",
        ] {
            assert_eq!(
                decode_wire_json::<serde_json::Value>(malformed)
                    .unwrap_err()
                    .code(),
                "FDB_STORAGE",
                "{malformed}"
            );
        }
    }
}
