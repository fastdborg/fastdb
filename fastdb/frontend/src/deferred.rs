//! Recognize documented future statement forms without reserving SQL names.
use crate::{Error, Result};
use fastql_parser::{Kind, Statement};

pub(crate) fn check(sql: &str) -> Result<()> {
    let tokens = fastql_parser::tokenize(sql)?;
    let prefix = |words: &[&str]| {
        words.iter().enumerate().all(|(i, word)| {
            tokens
                .get(i)
                .is_some_and(|t| t.kind == Kind::Word && t.text.eq_ignore_ascii_case(word))
        })
    };
    let feature = if prefix(&["DEFINE", "RELATION"]) {
        Some("inverse relations are deferred to V2")
    } else if prefix(&["CREATE", "SEARCH", "INDEX"]) {
        Some("search indexes are deferred to V2")
    } else if prefix(&["CREATE", "FUNCTION"]) {
        Some("user functions are deferred to V2")
    } else if prefix(&["DEFINE", "CHANGEFEED"])
        || prefix(&["REMOVE", "CHANGEFEED"])
        || prefix(&["SHOW", "CHANGES"])
    {
        Some("changefeeds are deferred to V3")
    } else if (prefix(&["LET"]) && tokens.get(1).is_some_and(|t| t.kind == Kind::Parameter))
        || (prefix(&["DO"])
            && tokens
                .get(1)
                .is_some_and(|t| t.kind == Kind::Symbol && t.text == "{"))
    {
        Some("procedural scripting is deferred to V3")
    } else if prefix(&["SELECT"])
        && tokens
            .iter()
            .find(|t| t.kind == Kind::Symbol && t.text == "{")
            .is_some_and(|brace| {
                matches!(
                    fastql_parser::parse(sql[..brace.start].trim()),
                    Ok(Statement::SelectRecord(_))
                )
            })
    {
        Some("record brace projections are deferred to V2")
    } else {
        None
    };
    match feature {
        Some(message) => Err(Error::Unsupported(message.into())),
        None => Ok(()),
    }
}
