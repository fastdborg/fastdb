//! Phase 0 recursive-descent parser.
//!
//! Parses exactly the four supported forms and rejects everything else,
//! including recognized-but-unimplemented clauses, with explicit errors.
//! Multiple statements are rejected. The parser depends only on this
//! crate; it never touches Turso AST.

use crate::ast::*;
use crate::error::{ParseError, ParseErrorKind};
use crate::lexer::{tokenize, Token, TokenKind};

/// Parse one Phase 0 statement from `input`.
///
/// `input` must be valid UTF-8 (`&str`). Callers receiving bytes must
/// validate UTF-8 before calling; invalid UTF-8 is outside this API's
/// contract and is reported by the caller, not synthesized here.
pub fn parse(input: &str) -> Result<Statement, ParseError> {
    let tokens = tokenize(input)?;
    let mut p = Parser { tokens, pos: 0 };
    p.parse_statement()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }
    fn peek_span(&self) -> Span {
        self.tokens[self.pos].span
    }
    fn prev_end(&self) -> usize {
        self.tokens[self.pos - 1].span.end()
    }
    fn at_end(&self) -> bool {
        matches!(self.tokens[self.pos].kind, TokenKind::Eof)
    }
    fn eat(&mut self, kind: &TokenKind) -> bool {
        if &self.tokens[self.pos].kind == kind {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, kind: &TokenKind, name: &'static str) -> Result<(), ParseError> {
        if &self.tokens[self.pos].kind == kind {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.unexpected(name))
        }
    }
    fn unexpected(&self, expected: &'static str) -> ParseError {
        let tok = &self.tokens[self.pos];
        ParseError::new(
            ParseErrorKind::UnexpectedToken {
                expected,
                found: tok.kind.describe(),
            },
            tok.span,
        )
    }
    fn expect_ident(&mut self, what: &'static str) -> Result<Identifier, ParseError> {
        let tok = &self.tokens[self.pos];
        if let TokenKind::Ident(s) = &tok.kind {
            let id = Identifier::new(s.clone(), tok.span);
            self.pos += 1;
            Ok(id)
        } else {
            Err(self.unexpected(what))
        }
    }
    fn parse_string_lit(&mut self) -> Result<StringLit, ParseError> {
        let tok = &self.tokens[self.pos];
        match &tok.kind {
            TokenKind::String(s) => {
                let lit = StringLit {
                    value: s.clone(),
                    span: tok.span,
                };
                self.pos += 1;
                Ok(lit)
            }
            _ => Err(self.unexpected("a string literal")),
        }
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        if matches!(self.peek_kind(), TokenKind::Eof) {
            return Err(ParseError::new(
                ParseErrorKind::EmptyInput,
                self.peek_span(),
            ));
        }
        let stmt = match self.peek_kind() {
            TokenKind::Create => Statement::Create(self.parse_create()?),
            TokenKind::Select => Statement::Select(self.parse_select()?),
            TokenKind::Delete => Statement::Delete(self.parse_delete()?),
            _ => return Err(self.unexpected("a statement keyword (CREATE, SELECT, or DELETE)")),
        };
        // One optional trailing semicolon.
        self.eat(&TokenKind::Semicolon);
        if !self.at_end() {
            let is_stmt_kw = matches!(
                self.peek_kind(),
                TokenKind::Create | TokenKind::Select | TokenKind::Delete
            );
            return Err(ParseError::new(
                if is_stmt_kw {
                    ParseErrorKind::MultipleStatements
                } else {
                    ParseErrorKind::TrailingTokens
                },
                self.peek_span(),
            ));
        }
        Ok(stmt)
    }

    fn parse_create(&mut self) -> Result<CreateStatement, ParseError> {
        let start = self.peek_span().offset;
        self.expect(&TokenKind::Create, "keyword CREATE")?;
        let target = self.parse_target()?;
        if target.id.is_none() {
            return Err(unsupported(
                "CREATE without an explicit record id (generated ids are not supported)",
                target.span,
            ));
        }
        self.expect(&TokenKind::Set, "keyword SET")?;
        let assignment = self.parse_assignment()?;
        let span = Span::new(start, self.prev_end() - start);
        Ok(CreateStatement {
            span,
            target,
            assignment,
        })
    }

    fn parse_assignment(&mut self) -> Result<Assignment, ParseError> {
        let field = self.expect_ident("a field name")?;
        self.expect(&TokenKind::Eq, "'='")?;
        let value = self.parse_string_lit()?;
        let span = field.span.union(value.span);
        // Phase 0 supports exactly one assignment; a comma means more follow.
        if matches!(self.peek_kind(), TokenKind::Comma) {
            return Err(unsupported(
                "multiple SET assignments are not supported",
                self.peek_span(),
            ));
        }
        Ok(Assignment { span, field, value })
    }

    fn parse_select(&mut self) -> Result<SelectStatement, ParseError> {
        let start = self.peek_span().offset;
        self.expect(&TokenKind::Select, "keyword SELECT")?;
        self.expect(&TokenKind::Star, "'*' (field projection is not supported)")?;
        self.expect(&TokenKind::From, "keyword FROM")?;
        let target = self.parse_target()?;
        let filter = if self.eat(&TokenKind::Where) {
            Some(self.parse_predicate()?)
        } else {
            None
        };
        // Enforce the two supported SELECT shapes.
        match (&target.id, &filter) {
            (Some(_), Some(f)) => {
                let span = match f {
                    Predicate::StringEquals { span, .. } => *span,
                };
                return Err(unsupported(
                    "a record-id SELECT combined with a WHERE filter is not supported",
                    span,
                ));
            }
            (None, None) => {
                return Err(unsupported(
                    "SELECT without a record id or WHERE filter is not supported",
                    target.span,
                ))
            }
            _ => {}
        }
        let span = Span::new(start, self.prev_end() - start);
        Ok(SelectStatement {
            span,
            target,
            filter,
        })
    }

    fn parse_predicate(&mut self) -> Result<Predicate, ParseError> {
        let start = self.peek_span().offset;
        let field = self.expect_ident("a field name")?;
        self.expect(&TokenKind::Eq, "'='")?;
        let value = self.parse_string_lit()?;
        let span = Span::new(start, self.prev_end() - start);
        Ok(Predicate::StringEquals { span, field, value })
    }

    fn parse_delete(&mut self) -> Result<DeleteStatement, ParseError> {
        let start = self.peek_span().offset;
        self.expect(&TokenKind::Delete, "keyword DELETE")?;
        if matches!(self.peek_kind(), TokenKind::From) {
            return Err(unsupported(
                "DELETE FROM is not supported (use `DELETE <table>:<id>`)",
                self.peek_span(),
            ));
        }
        let target = self.parse_target()?;
        if target.id.is_none() {
            return Err(unsupported(
                "DELETE without an explicit record id is not supported",
                target.span,
            ));
        }
        if matches!(self.peek_kind(), TokenKind::Where) {
            return Err(unsupported(
                "DELETE with a WHERE clause is not supported",
                self.peek_span(),
            ));
        }
        let span = Span::new(start, self.prev_end() - start);
        Ok(DeleteStatement { span, target })
    }

    fn parse_target(&mut self) -> Result<RecordTarget, ParseError> {
        let table = self.expect_ident("a table name")?;
        let start = table.span.offset;
        let id = if self.eat(&TokenKind::Colon) {
            Some(self.parse_record_id_part()?)
        } else {
            None
        };
        let span = Span::new(start, self.prev_end() - start);
        Ok(RecordTarget { span, table, id })
    }

    /// Phase 0 supports only a bare-identifier record id (`table:identifier`).
    /// A quoted string id is not part of the declared compatibility subset
    /// (`COMPAT.md` `RID-STR`) and is rejected here.
    fn parse_record_id_part(&mut self) -> Result<RecordIdPart, ParseError> {
        let tok = &self.tokens[self.pos];
        match &tok.kind {
            TokenKind::Ident(s) => {
                let part = RecordIdPart {
                    value: s.clone(),
                    span: tok.span,
                };
                self.pos += 1;
                Ok(part)
            }
            TokenKind::String(_) => Err(unsupported(
                "quoted record ids are not supported (use a bare identifier)",
                tok.span,
            )),
            _ => Err(self.unexpected("a record id (a bare identifier)")),
        }
    }
}

fn unsupported(what: &'static str, span: Span) -> ParseError {
    ParseError::new(ParseErrorKind::UnsupportedSyntax { what }, span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Statement;

    fn unsupported_kind(e: &ParseError) -> &'static str {
        match &e.kind {
            ParseErrorKind::UnsupportedSyntax { what } => what,
            _ => panic!("expected UnsupportedSyntax, got {:?}", e.kind),
        }
    }

    #[test]
    fn parse_create_exact() {
        //        CREATE person:tobie SET name = 'Tobie';
        let s = "CREATE person:tobie SET name = 'Tobie';";
        let stmt = parse(s).unwrap();
        let Statement::Create(c) = stmt else {
            panic!("expected Create");
        };
        assert_eq!(c.target.table.value, "person");
        assert_eq!(c.target.table.span, Span::new(7, 6)); // "person"
        let id = c.target.id.expect("id present");
        assert_eq!(id.value, "tobie");
        assert_eq!(id.span, Span::new(14, 5)); // "tobie"
        assert_eq!(c.assignment.field.value, "name");
        assert_eq!(c.assignment.value.value, "Tobie");
        assert_eq!(c.span, Span::new(0, s.len() - 1)); // excludes ';'
    }

    #[test]
    fn parse_select_record_exact() {
        let s = "SELECT * FROM person:tobie";
        let stmt = parse(s).unwrap();
        let Statement::Select(sel) = stmt else {
            panic!("expected Select");
        };
        assert_eq!(sel.target.table.value, "person");
        assert_eq!(sel.target.id.as_ref().unwrap().value, "tobie");
        assert!(sel.filter.is_none());
    }

    #[test]
    fn parse_select_filter_exact() {
        let s = "SELECT * FROM person WHERE name = 'Tobie'";
        let stmt = parse(s).unwrap();
        let Statement::Select(sel) = stmt else {
            panic!("expected Select");
        };
        assert_eq!(sel.target.table.value, "person");
        assert!(sel.target.id.is_none());
        match sel.filter.unwrap() {
            Predicate::StringEquals { field, value, .. } => {
                assert_eq!(field.value, "name");
                assert_eq!(value.value, "Tobie");
            }
        }
    }

    #[test]
    fn parse_delete_exact() {
        let s = "DELETE person:tobie";
        let stmt = parse(s).unwrap();
        let Statement::Delete(d) = stmt else {
            panic!("expected Delete");
        };
        assert_eq!(d.target.table.value, "person");
        assert_eq!(d.target.id.as_ref().unwrap().value, "tobie");
    }

    #[test]
    fn case_insensitive_keywords_preserve_ident_text() {
        let s = "create Person:Tobie set Name = 'value'";
        let stmt = parse(s).unwrap();
        let Statement::Create(c) = stmt else {
            panic!("expected Create");
        };
        // Keywords matched case-insensitively; identifiers retain text.
        assert_eq!(c.target.table.value, "Person");
        assert_eq!(c.target.id.unwrap().value, "Tobie");
        assert_eq!(c.assignment.field.value, "Name");
    }

    #[test]
    fn unicode_identifier_retained() {
        let s = "CREATE café:naïve SET flavor = 'good'";
        let stmt = parse(s).unwrap();
        let Statement::Create(c) = stmt else {
            panic!("expected Create");
        };
        assert_eq!(c.target.table.value, "café");
        assert_eq!(c.target.id.unwrap().value, "naïve");
    }

    #[test]
    fn whitespace_around_punctuation() {
        let s = "CREATE   person : tobie  SET  name  =  'Tobie' ;";
        let stmt = parse(s).unwrap();
        let Statement::Create(c) = stmt else {
            panic!("expected Create");
        };
        assert_eq!(c.target.id.unwrap().value, "tobie");
        assert_eq!(c.assignment.value.value, "Tobie");
    }

    #[test]
    fn string_with_escaped_quote() {
        // 'O''Brien' decodes to O'Brien; backslash is literal.
        let s = r"CREATE p:x SET n = 'O''Brien\n'";
        let stmt = parse(s).unwrap();
        let Statement::Create(c) = stmt else {
            panic!("expected Create");
        };
        assert_eq!(c.assignment.value.value, "O'Brien\\n");
    }

    #[test]
    fn semicolon_and_quote_inside_string_are_data() {
        // The value contains a semicolon and a quote; it must be one value
        // and there must be no second statement.
        let s = "CREATE p:x SET n = 'a;''b'";
        let stmt = parse(s).unwrap();
        let Statement::Create(c) = stmt else {
            panic!("expected Create");
        };
        assert_eq!(c.assignment.value.value, "a;'b");
    }

    #[test]
    fn unterminated_string() {
        let err = parse("CREATE p:x SET n = 'oops").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::UnterminatedString);
    }

    #[test]
    fn missing_table() {
        let err = parse("CREATE SET n = 'v'").unwrap_err();
        assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    }

    #[test]
    fn missing_id_in_create() {
        let err = parse("CREATE person SET n = 'v'").unwrap_err();
        assert!(matches!(err.kind, ParseErrorKind::UnsupportedSyntax { .. }));
    }

    #[test]
    fn missing_field() {
        let err = parse("CREATE p:x SET = 'v'").unwrap_err();
        assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    }

    #[test]
    fn missing_value() {
        let err = parse("CREATE p:x SET n = ").unwrap_err();
        assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    }

    #[test]
    fn missing_from() {
        let err = parse("SELECT * person").unwrap_err();
        assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    }

    #[test]
    fn trailing_unsupported_clause_return_only() {
        let err = parse("CREATE p:x SET n = 'v' RETURN NONE").unwrap_err();
        // RETURN lexes as an identifier -> trailing tokens.
        assert!(
            matches!(
                err.kind,
                ParseErrorKind::TrailingTokens | ParseErrorKind::UnexpectedToken { .. }
            ),
            "got {:?}",
            err.kind
        );
    }

    #[test]
    fn only_clause_unsupported() {
        let err = parse("SELECT ONLY * FROM p:x").unwrap_err();
        assert!(matches!(err.kind, ParseErrorKind::UnexpectedToken { .. }));
    }

    #[test]
    fn limit_clause_unsupported() {
        let err = parse("SELECT * FROM p:x LIMIT 5").unwrap_err();
        // LIMIT lexes as ident; '5' is not a valid token start -> error.
        assert!(!matches!(err.kind, ParseErrorKind::EmptyInput));
    }

    #[test]
    fn multiple_set_assignments_unsupported() {
        let err = parse("CREATE p:x SET a = '1', b = '2'").unwrap_err();
        assert!(unsup_contains(&err, "multiple SET assignments"));
    }

    #[test]
    fn multiple_statements_rejected() {
        let err = parse("CREATE p:x SET n = 'v'; CREATE p:y SET n = 'w'").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::MultipleStatements);
    }

    #[test]
    fn trailing_tokens_rejected() {
        let err = parse("DELETE p:x extra").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::TrailingTokens);
    }

    #[test]
    fn record_select_with_where_unsupported() {
        let err = parse("SELECT * FROM p:x WHERE n = 'v'").unwrap_err();
        assert!(unsup_contains(
            &err,
            "record-id SELECT combined with a WHERE"
        ));
    }

    #[test]
    fn bare_table_select_unsupported() {
        let err = parse("SELECT * FROM person").unwrap_err();
        assert!(unsup_contains(&err, "without a record id or WHERE filter"));
    }

    #[test]
    fn delete_from_unsupported() {
        let err = parse("DELETE FROM p:x").unwrap_err();
        assert!(unsup_contains(&err, "DELETE FROM"));
    }

    #[test]
    fn delete_without_id_unsupported() {
        let err = parse("DELETE person").unwrap_err();
        assert!(unsup_contains(&err, "explicit record id"));
    }

    #[test]
    fn numeric_record_id_unsupported() {
        // '5' is not a valid token start -> explicit lex error, never accepted.
        let err = parse("CREATE p:5 SET n = 'v'").unwrap_err();
        assert!(matches!(err.kind, ParseErrorKind::UnexpectedChar { .. }));
    }

    #[test]
    fn quoted_record_id_unsupported() {
        // Quoted ids are not part of the Phase 0 declared subset (COMPAT RID-STR).
        let err = parse("CREATE person:'tobie' SET name = 'Tobie'").unwrap_err();
        assert!(matches!(err.kind, ParseErrorKind::UnsupportedSyntax { .. }));
    }

    #[test]
    fn empty_input_no_panic() {
        let err = parse("").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::EmptyInput);
        let err = parse("   \n\t  ").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::EmptyInput);
    }

    #[test]
    fn arbitrary_bytes_smoke_no_panic() {
        // Random-ish bytes that are valid UTF-8 must not panic; they error.
        for s in [
            "\0",
            "\x01\x02\x03",
            "CREATE",
            ":::::",
            "'''",
            "SELECT * * FROM",
            "🦀🦀🦀",
            "CREATE p:x SET n = '",
            "\n\n\n;;;",
        ] {
            let _ = parse(s);
        }
    }

    #[test]
    fn optional_semicolon_accepted() {
        assert!(parse("DELETE p:x").is_ok());
        assert!(parse("DELETE p:x;").is_ok());
        assert!(parse("DELETE p:x ;").is_ok());
    }

    fn unsup_contains(e: &ParseError, needle: &str) -> bool {
        match &e.kind {
            ParseErrorKind::UnsupportedSyntax { what } => what.contains(needle),
            _ => {
                let _ = unsupported_kind(e);
                false
            }
        }
    }
}
