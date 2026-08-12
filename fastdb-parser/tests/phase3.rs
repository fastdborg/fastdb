#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb_parser::{
    parse, LimitKind, ParseErrorKind, ParserLimits, Statement, StatementCursor,
};

#[test]
fn p3_parse_001_cursor_defers_later_lexical_failure_and_keeps_global_spans() {
    let source = "CREATE person:a SET n=1; SELECT * FROM person:a; @";
    let mut cursor = StatementCursor::new(source);
    let first = cursor.next_statement().unwrap().unwrap();
    let second = cursor.next_statement().unwrap().unwrap();
    assert!(matches!(first, Statement::Create(_)));
    assert!(matches!(second, Statement::Select(_)));
    assert_eq!(first.span().offset, 0);
    assert_eq!(second.span().offset, source.find("SELECT").unwrap());
    let error = cursor.next_statement().unwrap_err();
    assert_eq!(error.span.offset, source.find('@').unwrap());
    assert!(matches!(
        error.kind,
        ParseErrorKind::UnexpectedCharacter { ch: '@' }
    ));
}

#[test]
fn p3_parse_002_cursor_handles_comments_trailing_separator_and_later_parse_error() {
    let source = "CREATE note:a SET text=';'; /* ; */ SELECT * FROM note:a; SELECT FROM";
    let mut cursor = StatementCursor::new(source);
    assert!(matches!(
        cursor.next_statement().unwrap(),
        Some(Statement::Create(_))
    ));
    assert!(matches!(
        cursor.next_statement().unwrap(),
        Some(Statement::Select(_))
    ));
    let error = cursor.next_statement().unwrap_err();
    assert!(error.span.offset >= source.rfind("FROM").unwrap());

    let mut trailing = StatementCursor::new("BEGIN; -- done\n");
    assert!(matches!(
        trailing.next_statement().unwrap(),
        Some(Statement::Begin(_))
    ));
    assert!(trailing.next_statement().unwrap().is_none());
}

#[test]
fn p3_parse_003_cursor_limits_are_cumulative_and_parse_consumes_it() {
    let limits = ParserLimits {
        max_statements: 2,
        ..ParserLimits::default()
    };
    let mut cursor = StatementCursor::with_limits("BEGIN; COMMIT; CANCEL", limits);
    cursor.next_statement().unwrap().unwrap();
    cursor.next_statement().unwrap().unwrap();
    let error = cursor.next_statement().unwrap_err();
    assert!(matches!(
        error.kind,
        ParseErrorKind::LimitExceeded {
            kind: LimitKind::Statements,
            limit: 2
        }
    ));

    let script = parse("BEGIN; COMMIT; CANCEL;").unwrap();
    assert_eq!(script.statements.len(), 3);
    assert_eq!(script.span.offset, 0);
    assert_eq!(script.span.end(), "BEGIN; COMMIT; CANCEL".len());

    let token_limits = ParserLimits {
        max_tokens: 4,
        ..ParserLimits::default()
    };
    let mut cursor = StatementCursor::with_limits("BEGIN; COMMIT; CANCEL", token_limits);
    cursor.next_statement().unwrap().unwrap();
    cursor.next_statement().unwrap().unwrap();
    assert!(matches!(
        cursor.next_statement().unwrap_err().kind,
        ParseErrorKind::LimitExceeded {
            kind: LimitKind::Tokens,
            limit: 4
        }
    ));

    let input_limits = ParserLimits {
        max_input_bytes: "BEGIN;".len(),
        ..ParserLimits::default()
    };
    let mut cursor = StatementCursor::with_limits("BEGIN; COMMIT", input_limits);
    cursor.next_statement().unwrap().unwrap();
    assert!(matches!(
        cursor.next_statement().unwrap_err().kind,
        ParseErrorKind::LimitExceeded {
            kind: LimitKind::InputBytes,
            limit: 6
        }
    ));
}
