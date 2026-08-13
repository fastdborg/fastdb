//! Bounded parsing and Unicode-aware string built-ins.

use crate::builtins::{
    StringBuiltin, StringDistance, StringSemver, StringSimilarity, StringUrlPart, StringValidator,
};
use crate::decode::{DatetimeValue, Value};
use crate::error::{FastDbError, Result};
use semver::{BuildMetadata, Prerelease, Version};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization as _};
use unicode_segmentation::UnicodeSegmentation as _;
use url::Url as ParsedUrl;

const MAX_STRING_BYTES: usize = 16 * 1024 * 1024;
const MAX_STRING_ITEMS: usize = 65_536;

pub(crate) fn evaluate(function: StringBuiltin, arguments: &[Value]) -> Result<Value> {
    use StringBuiltin::*;
    match function {
        Capitalize => bounded_string(capitalize(string(&arguments[0], "string::capitalize")?)),
        Concat => {
            let mut output = String::new();
            for value in arguments {
                append_bounded(&mut output, &crate::eval::render_string(value.clone())?)?;
            }
            Ok(Value::Str(output))
        }
        Contains => Ok(Value::Bool(
            string(&arguments[0], "string::contains")?
                .contains(string(&arguments[1], "string::contains")?),
        )),
        Distance(distance) => evaluate_distance(distance, arguments),
        EndsWith => Ok(Value::Bool(
            string(&arguments[0], "string::ends_with")?
                .ends_with(string(&arguments[1], "string::ends_with")?),
        )),
        HtmlEncode => bounded_string(html_encode(string(&arguments[0], "string::html::encode")?)),
        Join => {
            let separator = string(&arguments[0], "string::join")?;
            let mut output = String::new();
            for (index, value) in arguments[1..].iter().enumerate() {
                if index != 0 {
                    append_bounded(&mut output, separator)?;
                }
                append_bounded(&mut output, &crate::eval::render_string(value.clone())?)?;
            }
            Ok(Value::Str(output))
        }
        Len => Ok(Value::Integer(
            string(&arguments[0], "string::len")?
                .graphemes(true)
                .count() as i64,
        )),
        Lowercase => bounded_string(string(&arguments[0], "string::lowercase")?.to_lowercase()),
        Matches => {
            let input = string(&arguments[0], "string::matches")?;
            let Value::Regex(pattern) = &arguments[1] else {
                return Err(argument_type("string::matches", "a regex second argument"));
            };
            let regex = regex::Regex::new(pattern.as_str())
                .map_err(|_| FastDbError::Schema("regex pattern is invalid".into()))?;
            Ok(Value::Bool(regex.is_match(input)))
        }
        ParseEmailHost | ParseEmailUser => {
            let input = string(&arguments[0], "parse::email")?;
            let Some((user, host)) = parse_email(input) else {
                return Ok(Value::None);
            };
            Ok(Value::Str(if function == ParseEmailHost {
                host.to_string()
            } else {
                user.to_string()
            }))
        }
        ParseUrl(part) => evaluate_url_part(part, &arguments[0]),
        Repeat => {
            let input = string(&arguments[0], "string::repeat")?;
            let count = nonnegative_usize(&arguments[1], "string::repeat")?;
            let bytes = input.len().checked_mul(count).ok_or_else(output_limit)?;
            if bytes > MAX_STRING_BYTES {
                return Err(output_limit());
            }
            Ok(Value::Str(input.repeat(count)))
        }
        Replace => bounded_string(string(&arguments[0], "string::replace")?.replace(
            string(&arguments[1], "string::replace")?,
            string(&arguments[2], "string::replace")?,
        )),
        Reverse => bounded_string(
            string(&arguments[0], "string::reverse")?
                .graphemes(true)
                .rev()
                .collect(),
        ),
        Semver(operation) => evaluate_semver(operation, arguments),
        Similarity(operation) => evaluate_similarity(operation, arguments),
        Slice => evaluate_slice(arguments),
        Slug => bounded_string(slug(string(&arguments[0], "string::slug")?)),
        Split => {
            let input = string(&arguments[0], "string::split")?;
            let separator = string(&arguments[1], "string::split")?;
            let values: Vec<_> = input
                .split(separator)
                .map(|value| Value::Str(value.to_string()))
                .collect();
            if values.len() > MAX_STRING_ITEMS {
                return Err(FastDbError::ResourceLimit(
                    "string split exceeds the collection limit".into(),
                ));
            }
            Ok(Value::Array(values))
        }
        StartsWith => Ok(Value::Bool(
            string(&arguments[0], "string::starts_with")?
                .starts_with(string(&arguments[1], "string::starts_with")?),
        )),
        Trim => Ok(Value::Str(
            string(&arguments[0], "string::trim")?.trim().to_string(),
        )),
        Uppercase => bounded_string(string(&arguments[0], "string::uppercase")?.to_uppercase()),
        Validate(validator) => Ok(Value::Bool(validate(
            validator,
            string(&arguments[0], "string validator")?,
        ))),
        Words => {
            let values: Vec<_> = string(&arguments[0], "string::words")?
                .split_whitespace()
                .map(|value| Value::Str(value.to_string()))
                .collect();
            if values.len() > MAX_STRING_ITEMS {
                return Err(FastDbError::ResourceLimit(
                    "string words exceed the collection limit".into(),
                ));
            }
            Ok(Value::Array(values))
        }
    }
}

fn evaluate_distance(distance: StringDistance, arguments: &[Value]) -> Result<Value> {
    let left = string(&arguments[0], "string distance")?;
    let right = string(&arguments[1], "string distance")?;
    if left.len().saturating_add(right.len()) > MAX_STRING_BYTES {
        return Err(output_limit());
    }
    let value = match distance {
        StringDistance::DamerauLevenshtein => {
            Value::Integer(strsim::damerau_levenshtein(left, right) as i64)
        }
        StringDistance::Hamming => Value::Integer(
            strsim::hamming(left, right)
                .map_err(|_| FastDbError::Schema("hamming strings must have equal length".into()))?
                as i64,
        ),
        StringDistance::Levenshtein => Value::Integer(strsim::levenshtein(left, right) as i64),
        StringDistance::NormalizedDamerauLevenshtein => {
            Value::Float(strsim::normalized_damerau_levenshtein(left, right))
        }
        StringDistance::NormalizedLevenshtein => {
            Value::Float(strsim::normalized_levenshtein(left, right))
        }
        StringDistance::Osa => Value::Integer(strsim::osa_distance(left, right) as i64),
    };
    Ok(value)
}

fn evaluate_similarity(similarity: StringSimilarity, arguments: &[Value]) -> Result<Value> {
    let left = string(&arguments[0], "string similarity")?;
    let right = string(&arguments[1], "string similarity")?;
    let value = match similarity {
        StringSimilarity::Jaro => strsim::jaro(left, right),
        StringSimilarity::JaroWinkler => strsim::jaro_winkler(left, right),
    };
    Ok(Value::Float(value))
}

fn evaluate_semver(operation: StringSemver, arguments: &[Value]) -> Result<Value> {
    let mut version = Version::parse(string(&arguments[0], "string::semver")?)
        .map_err(|_| FastDbError::Schema("invalid semantic version".into()))?;
    use StringSemver::*;
    match operation {
        Compare => {
            let right = Version::parse(string(&arguments[1], "string::semver::compare")?)
                .map_err(|_| FastDbError::Schema("invalid semantic version".into()))?;
            Ok(Value::Integer(version.cmp(&right) as i64))
        }
        Major | Minor | Patch => Ok(Value::Integer(
            i64::try_from(match operation {
                Major => version.major,
                Minor => version.minor,
                Patch => version.patch,
                _ => unreachable!(),
            })
            .map_err(|_| FastDbError::Schema("semantic version component exceeds int".into()))?,
        )),
        IncMajor | IncMinor | IncPatch | SetMajor | SetMinor | SetPatch => {
            match operation {
                IncMajor => {
                    version.major = version.major.checked_add(1).ok_or_else(semver_overflow)?;
                    version.minor = 0;
                    version.patch = 0;
                }
                IncMinor => {
                    version.minor = version.minor.checked_add(1).ok_or_else(semver_overflow)?;
                    version.patch = 0;
                }
                IncPatch => {
                    version.patch = version.patch.checked_add(1).ok_or_else(semver_overflow)?;
                }
                SetMajor => version.major = semver_component(&arguments[1])?,
                SetMinor => version.minor = semver_component(&arguments[1])?,
                SetPatch => version.patch = semver_component(&arguments[1])?,
                _ => unreachable!(),
            }
            version.pre = Prerelease::EMPTY;
            version.build = BuildMetadata::EMPTY;
            Ok(Value::Str(version.to_string()))
        }
    }
}

fn evaluate_url_part(part: StringUrlPart, value: &Value) -> Result<Value> {
    let input = string(value, "parse::url")?;
    let Ok(url) = ParsedUrl::parse(input) else {
        return Ok(Value::None);
    };
    let result = match part {
        StringUrlPart::Domain => url.domain().map(str::to_string).map(Value::Str),
        StringUrlPart::Fragment => url.fragment().map(str::to_string).map(Value::Str),
        StringUrlPart::Host => url.host_str().map(str::to_string).map(Value::Str),
        StringUrlPart::Path => Some(Value::Str(url.path().to_string())),
        StringUrlPart::Port => url
            .port_or_known_default()
            .map(|port| Value::Integer(port.into())),
        StringUrlPart::Query => url.query().map(str::to_string).map(Value::Str),
        StringUrlPart::Scheme => Some(Value::Str(url.scheme().to_string())),
    };
    Ok(result.unwrap_or(Value::None))
}

fn evaluate_slice(arguments: &[Value]) -> Result<Value> {
    let graphemes: Vec<_> = string(&arguments[0], "string::slice")?
        .graphemes(true)
        .collect();
    let start = relative_bound(integer(&arguments[1], "string::slice")?, graphemes.len());
    let end = arguments
        .get(2)
        .map(|value| integer(value, "string::slice"))
        .transpose()?
        .map_or(graphemes.len(), |value| {
            relative_bound(value, graphemes.len())
        });
    if start >= end {
        return Ok(Value::Str(String::new()));
    }
    bounded_string(graphemes[start..end].concat())
}

fn validate(kind: StringValidator, input: &str) -> bool {
    use StringValidator::*;
    match kind {
        Alpha => !input.is_empty() && input.chars().all(char::is_alphabetic),
        Alphanumeric => !input.is_empty() && input.chars().all(char::is_alphanumeric),
        Ascii => input.is_ascii(),
        Datetime => DatetimeValue::parse(input).is_ok(),
        Domain => valid_domain(input),
        Email => parse_email(input).is_some(),
        Hexadecimal => {
            !input.is_empty() && input.chars().all(|character| character.is_ascii_hexdigit())
        }
        Ip => input.parse::<IpAddr>().is_ok(),
        Ipv4 => input.parse::<Ipv4Addr>().is_ok(),
        Ipv6 => input.parse::<Ipv6Addr>().is_ok(),
        Latitude => bounded_number(input, -90.0, 90.0),
        Longitude => bounded_number(input, -180.0, 180.0),
        Numeric => !input.is_empty() && input.chars().all(char::is_numeric),
        Record => input
            .split_once(':')
            .is_some_and(|(table, id)| valid_identifier(table) && !id.is_empty()),
        Semver => Version::parse(input).is_ok(),
        Ulid => valid_ulid(input),
        Url => ParsedUrl::parse(input).is_ok_and(|url| url.has_host()),
        Uuid => uuid::Uuid::parse_str(input).is_ok(),
    }
}

fn capitalize(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut capitalize_next = true;
    for character in input.chars() {
        if capitalize_next && !character.is_whitespace() {
            output.extend(character.to_uppercase());
            capitalize_next = false;
        } else {
            output.push(character);
        }
        if character.is_whitespace() {
            capitalize_next = true;
        }
    }
    output
}

fn html_encode(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            character if character.is_ascii_alphanumeric() => output.push(character),
            character if character.is_ascii() => {
                output.push_str("&#");
                output.push_str(&(character as u32).to_string());
                output.push(';');
            }
            character => output.push(character),
        }
    }
    output
}

fn slug(input: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in input
        .nfkd()
        .filter(|character| !is_combining_mark(*character))
    {
        if character.is_alphanumeric() {
            if separator && !output.is_empty() {
                output.push('-');
            }
            output.extend(character.to_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    output
}

fn parse_email(input: &str) -> Option<(&str, &str)> {
    let (user, host) = input.split_once('@')?;
    if user.is_empty()
        || user.len() > 64
        || host.is_empty()
        || input.matches('@').count() != 1
        || user.starts_with('.')
        || user.ends_with('.')
        || user.contains("..")
        || !user.chars().all(|character| {
            character.is_ascii_alphanumeric() || ".!#$%&'*+/=?^_`{|}~-".contains(character)
        })
        || !valid_domain(host)
    {
        None
    } else {
        Some((user, host))
    }
}

fn valid_domain(input: &str) -> bool {
    !input.is_empty()
        && input.len() <= 253
        && input.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
}

fn valid_identifier(input: &str) -> bool {
    let mut characters = input.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
}

fn valid_ulid(input: &str) -> bool {
    input.len() == 26
        && input.as_bytes()[0] <= b'7'
        && input.bytes().all(|byte| {
            matches!(byte.to_ascii_uppercase(), b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z')
        })
}

fn bounded_number(input: &str, minimum: f64, maximum: f64) -> bool {
    input
        .parse::<f64>()
        .is_ok_and(|value| value.is_finite() && (minimum..=maximum).contains(&value))
}

fn string<'a>(value: &'a Value, function: &str) -> Result<&'a str> {
    match value {
        Value::Str(value) => Ok(value),
        _ => Err(argument_type(function, "string arguments")),
    }
}

fn integer(value: &Value, function: &str) -> Result<i64> {
    match value {
        Value::Integer(value) => Ok(*value),
        _ => Err(argument_type(function, "integer index/count arguments")),
    }
}

fn nonnegative_usize(value: &Value, function: &str) -> Result<usize> {
    usize::try_from(integer(value, function)?)
        .map_err(|_| FastDbError::Schema(format!("{function} count must be nonnegative")))
}

fn semver_component(value: &Value) -> Result<u64> {
    u64::try_from(integer(value, "semantic version component")?)
        .map_err(|_| FastDbError::Schema("semantic version component must be nonnegative".into()))
}

fn relative_bound(value: i64, length: usize) -> usize {
    let length = i64::try_from(length).unwrap_or(i64::MAX);
    let value = if value < 0 {
        length.saturating_add(value)
    } else {
        value
    };
    value.clamp(0, length) as usize
}

fn append_bounded(output: &mut String, value: &str) -> Result<()> {
    if output.len().saturating_add(value.len()) > MAX_STRING_BYTES {
        Err(output_limit())
    } else {
        output.push_str(value);
        Ok(())
    }
}

fn bounded_string(value: String) -> Result<Value> {
    if value.len() > MAX_STRING_BYTES {
        Err(output_limit())
    } else {
        Ok(Value::Str(value))
    }
}

fn output_limit() -> FastDbError {
    FastDbError::ResourceLimit("string function output exceeds the byte limit".into())
}

fn semver_overflow() -> FastDbError {
    FastDbError::Schema("semantic version component overflow".into())
}

fn argument_type(function: &str, expected: &str) -> FastDbError {
    FastDbError::Schema(format!("{function} requires {expected}"))
}
