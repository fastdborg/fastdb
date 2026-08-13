//! Recursive-descent statement parser and Pratt expression parser.

use crate::ast::*;
use crate::error::{LimitKind, ParseError, ParseErrorKind};
use crate::lexer::{Lexer, Token, TokenKind};
use crate::ParserLimits;
use std::mem::discriminant;

pub fn parse(input: &str) -> Result<Script, ParseError> {
    parse_with_limits(input, &ParserLimits::default())
}

pub fn parse_with_limits(input: &str, limits: &ParserLimits) -> Result<Script, ParseError> {
    let mut cursor = StatementCursor::with_limits(input, limits.clone());
    let mut statements = Vec::new();
    while let Some(statement) = cursor.next_statement()? {
        statements.push(statement);
    }
    let first = statements.first().expect("empty input rejected").span();
    let last = statements.last().expect("empty input rejected").span();
    Ok(Script {
        statements,
        span: first.union(last),
    })
}

pub fn parse_one(input: &str) -> Result<Statement, ParseError> {
    parse_one_with_limits(input, &ParserLimits::default())
}

pub fn parse_one_with_limits(input: &str, limits: &ParserLimits) -> Result<Statement, ParseError> {
    let script = parse_with_limits(input, limits)?;
    if script.statements.len() != 1 {
        let span = script
            .statements
            .get(1)
            .map_or(script.span, Statement::span);
        return Err(ParseError::new(
            ParseErrorKind::MultipleStatements {
                count: script.statements.len(),
            },
            span,
        ));
    }
    Ok(script
        .statements
        .into_iter()
        .next()
        .expect("length checked above"))
}

/// Incrementally lexes and parses one statement at a time.
///
/// Spans always refer to the complete input, and token/statement limits are
/// cumulative across calls. In particular, a lexical error after a statement
/// separator is not discovered until the caller asks for the later statement.
pub struct StatementCursor<'a> {
    source: &'a str,
    position: usize,
    limits: ParserLimits,
    token_count: usize,
    statement_count: usize,
    emitted_statement: bool,
    finished: bool,
}

impl<'a> StatementCursor<'a> {
    pub fn new(source: &'a str) -> Self {
        Self::with_limits(source, ParserLimits::default())
    }

    pub fn with_limits(source: &'a str, limits: ParserLimits) -> Self {
        Self {
            source,
            position: 0,
            limits,
            token_count: 0,
            statement_count: 0,
            emitted_statement: false,
            finished: false,
        }
    }

    pub fn next_statement(&mut self) -> Result<Option<Statement>, ParseError> {
        if self.finished {
            return Ok(None);
        }

        let mut tokens = Vec::new();
        loop {
            let mut lexer = Lexer {
                source: self.source,
                position: self.position,
                limits: &self.limits,
            };
            let token = lexer.next_token()?;
            self.position = lexer.position;
            if self.position > self.limits.max_input_bytes {
                return Err(ParseError::new(
                    ParseErrorKind::LimitExceeded {
                        kind: LimitKind::InputBytes,
                        limit: self.limits.max_input_bytes,
                    },
                    Span::new(
                        self.limits.max_input_bytes,
                        self.position - self.limits.max_input_bytes,
                    ),
                ));
            }

            let eof = matches!(token.kind, TokenKind::Eof);
            if !eof {
                if self.token_count == self.limits.max_tokens {
                    return Err(ParseError::new(
                        ParseErrorKind::LimitExceeded {
                            kind: LimitKind::Tokens,
                            limit: self.limits.max_tokens,
                        },
                        token.span,
                    ));
                }
                self.token_count += 1;
            }
            let separator = matches!(token.kind, TokenKind::Semicolon);
            tokens.push(token);
            if eof || separator {
                let boundary = tokens.last().expect("token was pushed").span.end();
                if separator {
                    tokens.push(Token {
                        kind: TokenKind::Eof,
                        span: Span::new(boundary, 0),
                    });
                } else {
                    self.finished = true;
                }
                break;
            }
        }

        if matches!(
            tokens.first().map(|token| &token.kind),
            Some(TokenKind::Eof)
        ) {
            if self.emitted_statement {
                return Ok(None);
            }
            return Err(ParseError::new(ParseErrorKind::EmptyInput, tokens[0].span));
        }

        let mut script = Parser::new(tokens, &self.limits).parse_script()?;
        let statement = script
            .statements
            .pop()
            .expect("parser accepted exactly one cursor segment");
        self.statement_count += 1;
        if self.statement_count > self.limits.max_statements {
            return Err(ParseError::new(
                ParseErrorKind::LimitExceeded {
                    kind: LimitKind::Statements,
                    limit: self.limits.max_statements,
                },
                statement.span(),
            ));
        }
        self.emitted_statement = true;
        Ok(Some(statement))
    }
}

struct Parser<'a> {
    tokens: Vec<Token>,
    position: usize,
    limits: &'a ParserLimits,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: Vec<Token>, limits: &'a ParserLimits) -> Self {
        Self {
            tokens,
            position: 0,
            limits,
            depth: 0,
        }
    }

    fn parse_script(mut self) -> Result<Script, ParseError> {
        if self.at(&TokenKind::Eof) {
            return Err(ParseError::new(
                ParseErrorKind::EmptyInput,
                self.peek().span,
            ));
        }
        if self.at(&TokenKind::Semicolon) {
            return Err(ParseError::new(
                ParseErrorKind::EmptyStatement,
                self.peek().span,
            ));
        }

        let mut statements = Vec::new();
        loop {
            let statement = self.parse_statement()?;
            self.check_collection_limit(
                statements.len() + 1,
                LimitKind::Statements,
                self.limits.max_statements,
                statement.span(),
            )?;
            statements.push(statement);

            if self.at(&TokenKind::Eof) {
                break;
            }
            if !self.eat(&TokenKind::Semicolon) {
                return Err(ParseError::new(
                    ParseErrorKind::MissingStatementSeparator,
                    self.peek().span,
                ));
            }
            if self.at(&TokenKind::Eof) {
                break;
            }
            if self.at(&TokenKind::Semicolon) {
                return Err(ParseError::new(
                    ParseErrorKind::EmptyStatement,
                    self.peek().span,
                ));
            }
        }

        let first = statements.first().expect("empty input rejected").span();
        let last = statements.last().expect("empty input rejected").span();
        Ok(Script {
            statements,
            span: first.union(last),
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        let statement = match &self.peek().kind {
            TokenKind::Create if self.at_offset(1, &TokenKind::Index) => {
                Statement::DefineIndex(self.parse_create_index()?)
            }
            TokenKind::Create => Statement::Create(self.parse_create()?),
            TokenKind::Relate => Statement::Relate(self.parse_relate()?),
            TokenKind::Select => Statement::Select(self.parse_select()?),
            TokenKind::Update => Statement::Update(self.parse_update()?),
            TokenKind::Delete => Statement::Delete(self.parse_delete()?),
            TokenKind::Define => self.parse_define()?,
            TokenKind::Explain => Statement::Explain(self.parse_explain()?),
            TokenKind::Remove => Statement::RemoveIndex(self.parse_index_maintenance(false)?),
            TokenKind::Rebuild => Statement::RebuildIndex(self.parse_index_maintenance(true)?),
            TokenKind::Begin => Statement::Begin(self.parse_transaction(TokenKind::Begin)?),
            TokenKind::Commit => Statement::Commit(self.parse_transaction(TokenKind::Commit)?),
            TokenKind::Cancel => Statement::Cancel(self.parse_transaction(TokenKind::Cancel)?),
            kind if is_unsupported_statement(kind) => {
                return Err(ParseError::unsupported(
                    "statement family is outside the FastDB MVP",
                    self.peek().span,
                ));
            }
            TokenKind::Eof => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedEof {
                        expected: "a statement",
                    },
                    self.peek().span,
                ));
            }
            _ => return Err(self.unexpected("a statement keyword")),
        };
        self.ensure_statement_boundary()?;
        Ok(statement)
    }

    fn parse_create(&mut self) -> Result<CreateStatement, ParseError> {
        let start = self.expect(&TokenKind::Create, "keyword CREATE")?.span;
        let only = self.take(&TokenKind::Only).map(|token| token.span);
        let target = self.parse_target()?;
        let data = if self.eat(&TokenKind::Content) {
            CreateData::Content(self.parse_expression()?)
        } else if self.eat(&TokenKind::Set) {
            CreateData::Set(self.parse_assignments()?)
        } else {
            return Err(self.unexpected("keyword CONTENT or SET"));
        };
        if self.at(&TokenKind::Content) || self.at(&TokenKind::Set) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "CONTENT and SET are mutually exclusive",
                },
                self.peek().span,
            ));
        }
        let return_clause = if self.eat(&TokenKind::Return) {
            Some(self.parse_return_clause(ReturnContext::Create)?)
        } else {
            None
        };
        if self.at(&TokenKind::Return) {
            return Err(self.duplicate_clause("RETURN"));
        }
        let end = self.previous_end();
        Ok(CreateStatement {
            span: Span::new(start.offset, end - start.offset),
            only,
            target,
            data,
            return_clause,
        })
    }

    fn parse_relate(&mut self) -> Result<RelateStatement, ParseError> {
        let start = self.expect(&TokenKind::Relate, "keyword RELATE")?.span;
        let only = self.take(&TokenKind::Only).map(|token| token.span);
        let from = self.parse_prefix_expression()?;
        self.validate_relate_endpoint(&from)?;
        self.expect(&TokenKind::ForwardArrow, "'->' after the source record")?;
        let relation = self.expect_identifier("a relation table name")?;
        self.expect(&TokenKind::ForwardArrow, "'->' after the relation table")?;
        let to = self.parse_prefix_expression()?;
        self.validate_relate_endpoint(&to)?;

        let data = if self.eat(&TokenKind::Content) {
            Some(CreateData::Content(self.parse_expression()?))
        } else if self.eat(&TokenKind::Set) {
            Some(CreateData::Set(self.parse_assignments()?))
        } else {
            None
        };
        if self.at(&TokenKind::Or) {
            return Err(ParseError::unsupported(
                "RELATE OR UPDATE is outside the Phase 7 grammar",
                self.peek().span,
            ));
        }
        let return_clause = if self.eat(&TokenKind::Return) {
            Some(self.parse_return_clause(ReturnContext::Create)?)
        } else {
            None
        };
        if self.at(&TokenKind::Return) {
            return Err(self.duplicate_clause("RETURN"));
        }
        if self.at(&TokenKind::Timeout) {
            return Err(ParseError::unsupported(
                "RELATE TIMEOUT is outside the Phase 7 grammar",
                self.peek().span,
            ));
        }
        let end = self.previous_end();
        Ok(RelateStatement {
            span: Span::new(start.offset, end - start.offset),
            only,
            from,
            relation,
            to,
            data,
            return_clause,
        })
    }

    fn validate_relate_endpoint(&self, endpoint: &Expr) -> Result<(), ParseError> {
        if matches!(
            endpoint.kind,
            ExprKind::RecordId(_) | ExprKind::Parameter(_)
        ) {
            return Ok(());
        }
        Err(ParseError::unsupported(
            "RELATE endpoints must be record literals or bound record parameters",
            endpoint.span,
        ))
    }

    fn parse_select(&mut self) -> Result<SelectStatement, ParseError> {
        let start = self.expect(&TokenKind::Select, "keyword SELECT")?.span;
        let projections = self.parse_projections()?;
        self.expect(&TokenKind::From, "keyword FROM")?;
        let only = self.take(&TokenKind::Only).map(|token| token.span);
        let target = self.parse_target()?;
        let condition = if self.eat(&TokenKind::Where) {
            Some(self.parse_expression()?)
        } else {
            None
        };
        let order_by = if self.eat(&TokenKind::Order) {
            self.expect(&TokenKind::By, "keyword BY after ORDER")?;
            self.parse_order_by()?
        } else {
            Vec::new()
        };
        let limit = if self.eat(&TokenKind::Limit) {
            Some(self.parse_nonnegative_integer()?)
        } else {
            None
        };
        let start_value = if self.eat(&TokenKind::Start) {
            Some(self.parse_nonnegative_integer()?)
        } else {
            None
        };

        if self.at(&TokenKind::Where) {
            return Err(if condition.is_some() {
                self.duplicate_clause("WHERE")
            } else {
                self.out_of_order_clause("WHERE")
            });
        }
        if self.at(&TokenKind::Order) {
            return Err(if order_by.is_empty() {
                self.out_of_order_clause("ORDER BY")
            } else {
                self.duplicate_clause("ORDER BY")
            });
        }
        if self.at(&TokenKind::Limit) {
            return Err(if limit.is_some() {
                self.duplicate_clause("LIMIT")
            } else {
                self.out_of_order_clause("LIMIT")
            });
        }
        if self.at(&TokenKind::Start) {
            return Err(if start_value.is_some() {
                self.duplicate_clause("START")
            } else {
                self.out_of_order_clause("START")
            });
        }

        let end = self.previous_end();
        Ok(SelectStatement {
            span: Span::new(start.offset, end - start.offset),
            projections,
            only,
            target,
            condition,
            order_by,
            limit,
            start: start_value,
        })
    }

    fn parse_update(&mut self) -> Result<UpdateStatement, ParseError> {
        let start = self.expect(&TokenKind::Update, "keyword UPDATE")?.span;
        if self.at(&TokenKind::Only) {
            return Err(ParseError::unsupported(
                "UPDATE ONLY is outside the MVP grammar",
                self.peek().span,
            ));
        }
        let target = self.parse_target()?;
        if matches!(
            self.peek().kind,
            TokenKind::Content
                | TokenKind::Merge
                | TokenKind::Patch
                | TokenKind::Replace
                | TokenKind::Unset
        ) {
            return Err(ParseError::unsupported(
                "only UPDATE ... SET is in the MVP grammar",
                self.peek().span,
            ));
        }
        self.expect(&TokenKind::Set, "keyword SET")?;
        let assignments = self.parse_assignments()?;
        let condition = if self.eat(&TokenKind::Where) {
            Some(self.parse_expression()?)
        } else {
            None
        };
        let return_clause = if self.eat(&TokenKind::Return) {
            Some(self.parse_return_clause(ReturnContext::Update)?)
        } else {
            None
        };
        if self.at(&TokenKind::Where) {
            return Err(if condition.is_some() {
                self.duplicate_clause("WHERE")
            } else {
                self.out_of_order_clause("WHERE")
            });
        }
        if self.at(&TokenKind::Return) {
            return Err(self.duplicate_clause("RETURN"));
        }
        let end = self.previous_end();
        Ok(UpdateStatement {
            span: Span::new(start.offset, end - start.offset),
            target,
            assignments,
            condition,
            return_clause,
        })
    }

    fn parse_delete(&mut self) -> Result<DeleteStatement, ParseError> {
        let start = self.expect(&TokenKind::Delete, "keyword DELETE")?.span;
        if self.at(&TokenKind::From) || self.at(&TokenKind::Only) {
            return Err(ParseError::unsupported(
                "DELETE FROM and DELETE ONLY are outside the MVP grammar",
                self.peek().span,
            ));
        }
        let target = self.parse_target()?;
        let condition = if self.eat(&TokenKind::Where) {
            Some(self.parse_expression()?)
        } else {
            None
        };
        let return_clause = if self.eat(&TokenKind::Return) {
            Some(self.parse_return_clause(ReturnContext::Delete)?)
        } else {
            None
        };
        if self.at(&TokenKind::Where) {
            return Err(if condition.is_some() {
                self.duplicate_clause("WHERE")
            } else {
                self.out_of_order_clause("WHERE")
            });
        }
        if self.at(&TokenKind::Return) {
            return Err(self.duplicate_clause("RETURN"));
        }
        let end = self.previous_end();
        Ok(DeleteStatement {
            span: Span::new(start.offset, end - start.offset),
            target,
            condition,
            return_clause,
        })
    }

    fn parse_define(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(&TokenKind::Define, "keyword DEFINE")?.span;
        match &self.peek().kind {
            TokenKind::Table => Ok(Statement::DefineTable(self.parse_define_table(start)?)),
            TokenKind::Field => Ok(Statement::DefineField(self.parse_define_field(start)?)),
            TokenKind::Analyzer => Ok(Statement::DefineAnalyzer(
                self.parse_define_analyzer(start)?,
            )),
            TokenKind::Index => Ok(Statement::DefineIndex(self.parse_define_index(start)?)),
            _ => Err(ParseError::unsupported(
                "only DEFINE TABLE, DEFINE FIELD, DEFINE ANALYZER, and DEFINE INDEX are supported",
                self.peek().span,
            )),
        }
    }

    fn parse_explain(&mut self) -> Result<ExplainStatement, ParseError> {
        let start = self.expect(&TokenKind::Explain, "keyword EXPLAIN")?.span;
        if !self.at(&TokenKind::Select) {
            return Err(ParseError::unsupported(
                "Phase 6 EXPLAIN accepts SELECT only",
                self.peek().span,
            ));
        }
        let select = self.parse_select()?;
        Ok(ExplainStatement {
            span: start.union(select.span),
            select,
        })
    }

    fn parse_index_maintenance(
        &mut self,
        rebuild: bool,
    ) -> Result<IndexMaintenanceStatement, ParseError> {
        let keyword = if rebuild {
            TokenKind::Rebuild
        } else {
            TokenKind::Remove
        };
        let start = self.expect(&keyword, "index maintenance statement")?.span;
        self.expect(&TokenKind::Index, "keyword INDEX")?;
        let name = self.expect_identifier("an index name")?;
        self.expect(&TokenKind::On, "keyword ON")?;
        let table_keyword = self.take(&TokenKind::Table).map(|token| token.span);
        let table = self.expect_identifier("a table name")?;
        Ok(IndexMaintenanceStatement {
            span: start.union(table.span),
            name,
            table_keyword,
            table,
        })
    }

    fn parse_define_table(&mut self, start: Span) -> Result<DefineTableStatement, ParseError> {
        self.expect(&TokenKind::Table, "keyword TABLE")?;
        let name = self.expect_identifier("a table name")?;
        let mode = if let Some(token) = self.take(&TokenKind::Schemaless) {
            Spanned::new(TableMode::Schemaless, token.span)
        } else if let Some(token) = self.take(&TokenKind::Schemafull) {
            Spanned::new(TableMode::Schemafull, token.span)
        } else if self.at(&TokenKind::Type) {
            Spanned::new(TableMode::Schemaless, Span::new(name.span.end(), 0))
        } else {
            return Err(self.unexpected("SCHEMALESS, SCHEMAFULL, or TYPE"));
        };
        let kind = if let Some(type_token) = self.take(&TokenKind::Type) {
            if let Some(normal) = self.take(&TokenKind::Normal) {
                TableKindSyntax::Normal {
                    type_span: Some(type_token.span.union(normal.span)),
                }
            } else if let Some(relation) = self.take(&TokenKind::Relation) {
                let mut input = None;
                let mut output = None;
                let mut enforced = None;
                loop {
                    if self.at(&TokenKind::In) || self.at(&TokenKind::From) {
                        let clause = self.advance().clone();
                        if input.is_some() {
                            return Err(ParseError::new(
                                ParseErrorKind::DuplicateClause { clause: "IN/FROM" },
                                clause.span,
                            ));
                        }
                        input = Some(self.expect_identifier("an input endpoint table")?);
                    } else if self.at(&TokenKind::Out) || self.at(&TokenKind::To) {
                        let clause = self.advance().clone();
                        if output.is_some() {
                            return Err(ParseError::new(
                                ParseErrorKind::DuplicateClause { clause: "OUT/TO" },
                                clause.span,
                            ));
                        }
                        output = Some(self.expect_identifier("an output endpoint table")?);
                    } else if let Some(token) = self.take(&TokenKind::Enforced) {
                        if enforced.is_some() {
                            return Err(ParseError::new(
                                ParseErrorKind::DuplicateClause { clause: "ENFORCED" },
                                token.span,
                            ));
                        }
                        enforced = Some(token.span);
                    } else {
                        break;
                    }
                }
                let end = enforced
                    .or_else(|| output.as_ref().map(|value| value.span))
                    .or_else(|| input.as_ref().map(|value| value.span))
                    .unwrap_or(relation.span);
                TableKindSyntax::Relation(RelationTableType {
                    span: type_token.span.union(end),
                    input,
                    output,
                    enforced,
                })
            } else {
                return Err(self.unexpected("NORMAL or RELATION after TYPE"));
            }
        } else {
            TableKindSyntax::Normal { type_span: None }
        };
        let end = match &kind {
            TableKindSyntax::Normal {
                type_span: Some(span),
            } => span.end(),
            TableKindSyntax::Normal { type_span: None } => mode.span.end(),
            TableKindSyntax::Relation(relation) => relation.span.end(),
        };
        Ok(DefineTableStatement {
            span: Span::new(start.offset, end - start.offset),
            name,
            mode,
            kind,
        })
    }

    fn parse_define_field(&mut self, start: Span) -> Result<DefineFieldStatement, ParseError> {
        self.expect(&TokenKind::Field, "keyword FIELD")?;
        let path = self.parse_field_path()?;
        self.expect(&TokenKind::On, "keyword ON")?;
        let table_keyword = self.take(&TokenKind::Table).map(|token| token.span);
        let table = self.expect_identifier("a table name")?;
        self.expect(&TokenKind::Type, "keyword TYPE")?;
        let ty = self.parse_schema_type()?;
        Ok(DefineFieldStatement {
            span: Span::new(start.offset, ty.span.end() - start.offset),
            path,
            table_keyword,
            table,
            ty,
        })
    }

    fn parse_define_analyzer(
        &mut self,
        start: Span,
    ) -> Result<DefineAnalyzerStatement, ParseError> {
        self.expect(&TokenKind::Analyzer, "keyword ANALYZER")?;
        let name = self.expect_identifier("an analyzer name")?;
        self.expect(&TokenKind::Tokenizers, "keyword TOKENIZERS")?;
        let tokenizer_token = self.peek().clone();
        let tokenizer = match &tokenizer_token.kind {
            TokenKind::Ident(value) if value.eq_ignore_ascii_case("blank") => {
                self.position += 1;
                Spanned::new(AnalyzerTokenizerSyntax::Blank, tokenizer_token.span)
            }
            _ => {
                return Err(ParseError::unsupported(
                    "Phase 8 Surreal analyzers support exactly TOKENIZERS blank",
                    tokenizer_token.span,
                ));
            }
        };
        if matches!(
            self.peek().kind,
            TokenKind::Functions | TokenKind::Filters | TokenKind::Ident(_)
        ) {
            return Err(ParseError::unsupported(
                "analyzer functions and filters are outside the Phase 8 subset",
                self.peek().span,
            ));
        }
        Ok(DefineAnalyzerStatement {
            span: start.union(tokenizer.span),
            name,
            tokenizer,
        })
    }

    fn parse_define_index(&mut self, start: Span) -> Result<DefineIndexStatement, ParseError> {
        self.expect(&TokenKind::Index, "keyword INDEX")?;
        let name = self.expect_identifier("an index name")?;
        self.expect(&TokenKind::On, "keyword ON")?;
        let table_keyword = self.take(&TokenKind::Table).map(|token| token.span);
        let table = self.expect_identifier("a table name")?;
        self.expect(&TokenKind::Fields, "keyword FIELDS")?;
        let mut fields = vec![self.parse_field_path()?];
        while self.eat(&TokenKind::Comma) {
            let field = self.parse_field_path()?;
            self.check_element_count(fields.len() + 1, field.span)?;
            fields.push(field);
        }
        let unique = self.take(&TokenKind::Unique).map(|token| token.span);
        let kind = if let Some(fulltext) = self.take(&TokenKind::Fulltext) {
            if !self.eat(&TokenKind::Analyzer) {
                return Err(ParseError::unsupported(
                    "FULLTEXT indexes require an ANALYZER clause",
                    self.peek().span,
                ));
            }
            let analyzer = self.expect_identifier("an analyzer name")?;
            let highlights = self.take(&TokenKind::Highlights).map(|token| token.span);
            IndexKindSyntax::Fulltext {
                span: fulltext.span.union(highlights.unwrap_or(analyzer.span)),
                analyzer,
                highlights,
            }
        } else if let Some(using) = self.take(&TokenKind::Using) {
            let name = self.expect_identifier("an index provider name")?;
            let options = if self.eat(&TokenKind::With) {
                self.parse_index_options()?
            } else {
                Vec::new()
            };
            let end = options.last().map_or(name.span, |option| option.span);
            IndexKindSyntax::Provider {
                span: using.span.union(end),
                name,
                options,
            }
        } else {
            IndexKindSyntax::Btree
        };
        let kind_end = match &kind {
            IndexKindSyntax::Btree => None,
            IndexKindSyntax::Fulltext { span, .. } | IndexKindSyntax::Provider { span, .. } => {
                Some(span.end())
            }
        };
        let end = kind_end.unwrap_or_else(|| {
            unique.map_or_else(|| fields.last().expect("one field").span.end(), Span::end)
        });
        Ok(DefineIndexStatement {
            span: Span::new(start.offset, end - start.offset),
            name,
            table_keyword,
            table,
            fields,
            unique,
            kind,
            surface: IndexDefinitionSurface::SurrealDefine,
        })
    }

    fn parse_create_index(&mut self) -> Result<DefineIndexStatement, ParseError> {
        let start = self.expect(&TokenKind::Create, "keyword CREATE")?.span;
        self.expect(&TokenKind::Index, "keyword INDEX")?;
        let name = self.expect_identifier("an index name")?;
        self.expect(&TokenKind::On, "keyword ON")?;
        let table_keyword = self.take(&TokenKind::Table).map(|token| token.span);
        let table = self.expect_identifier("a table name")?;
        let using = self.expect(&TokenKind::Using, "keyword USING")?.span;
        let provider = self.expect_identifier("an index provider name")?;
        self.expect(&TokenKind::LeftParen, "'(' after index provider")?;
        let mut fields = vec![self.parse_field_path()?];
        while self.eat(&TokenKind::Comma) {
            let field = self.parse_field_path()?;
            self.check_element_count(fields.len() + 1, field.span)?;
            fields.push(field);
        }
        let close = self.expect(&TokenKind::RightParen, "')' after indexed fields")?;
        let options = if self.eat(&TokenKind::With) {
            self.parse_index_options()?
        } else {
            Vec::new()
        };
        let end = options.last().map_or(close.span, |option| option.span);
        Ok(DefineIndexStatement {
            span: start.union(end),
            name,
            table_keyword,
            table,
            fields,
            unique: None,
            kind: IndexKindSyntax::Provider {
                span: using.union(end),
                name: provider,
                options,
            },
            surface: IndexDefinitionSurface::FastDbCreate,
        })
    }

    fn parse_index_options(&mut self) -> Result<Vec<IndexOption>, ParseError> {
        self.expect(&TokenKind::LeftParen, "'(' after WITH")?;
        let mut options = Vec::new();
        if self.eat(&TokenKind::RightParen) {
            return Ok(options);
        }
        loop {
            let key = self.expect_identifier("an index option name")?;
            self.expect(&TokenKind::Equal, "'=' after index option name")?;
            let value = self.parse_expression()?;
            let span = key.span.union(value.span);
            self.check_element_count(options.len() + 1, span)?;
            options.push(IndexOption { span, key, value });
            if !self.eat(&TokenKind::Comma) {
                self.expect(&TokenKind::RightParen, "')' after index options")?;
                return Ok(options);
            }
            if self.eat(&TokenKind::RightParen) {
                return Ok(options);
            }
        }
    }

    fn parse_transaction(
        &mut self,
        keyword: TokenKind,
    ) -> Result<TransactionStatement, ParseError> {
        let token = self.expect(&keyword, "transaction statement")?;
        if self.at(&TokenKind::Transaction) {
            return Err(ParseError::unsupported(
                "the optional TRANSACTION suffix is outside the fixed MVP grammar",
                self.peek().span,
            ));
        }
        Ok(TransactionStatement { span: token.span })
    }

    fn parse_target(&mut self) -> Result<Target, ParseError> {
        let table = self.expect_identifier("a table name")?;
        if !self.eat(&TokenKind::Colon) {
            return Ok(Target::Table(TableTarget {
                span: table.span,
                name: table,
            }));
        }
        let id = self.parse_record_id_part()?;
        let span = table.span.union(id.span);
        Ok(Target::Record(RecordId { span, table, id }))
    }

    fn parse_record_id_part(&mut self) -> Result<RecordIdPart, ParseError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Ident(value) => {
                if value.eq_ignore_ascii_case("u") {
                    if let Some(string) = self.tokens.get(self.position + 1) {
                        if token.span.end() == string.span.offset {
                            if let TokenKind::String(value) = &string.kind {
                                let span = token.span.union(string.span);
                                let uuid = parse_uuid(value, span)?;
                                self.position += 2;
                                return Ok(RecordIdPart {
                                    span,
                                    kind: RecordIdPartKind::Uuid(uuid),
                                });
                            }
                        }
                    }
                }
                self.position += 1;
                Ok(RecordIdPart {
                    span: token.span,
                    kind: RecordIdPartKind::Bare(value),
                })
            }
            TokenKind::QuotedIdent(value) => {
                self.position += 1;
                Ok(RecordIdPart {
                    span: token.span,
                    kind: RecordIdPartKind::Quoted(value),
                })
            }
            TokenKind::Number(value) => {
                self.position += 1;
                let integer = parse_signed_integer(&value, 1, token.span)?;
                Ok(RecordIdPart {
                    span: token.span,
                    kind: RecordIdPartKind::Integer(integer),
                })
            }
            TokenKind::Plus | TokenKind::Minus => {
                self.position += 1;
                let sign = if matches!(token.kind, TokenKind::Minus) {
                    -1
                } else {
                    1
                };
                let number = self.peek().clone();
                let TokenKind::Number(value) = number.kind else {
                    return Err(self.unexpected("an integer record-ID component"));
                };
                self.position += 1;
                let span = token.span.union(number.span);
                let integer = parse_signed_integer(&value, sign, span)?;
                Ok(RecordIdPart {
                    span,
                    kind: RecordIdPartKind::Integer(integer),
                })
            }
            TokenKind::String(_) => Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "record-ID text uses backticks, not string quotes",
                },
                token.span,
            )),
            TokenKind::Eof => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: "a record-ID component",
                },
                token.span,
            )),
            _ => Err(self
                .unexpected("a bare, backtick-quoted, integer, or typed-UUID record-ID component")),
        }
    }

    fn parse_assignments(&mut self) -> Result<Vec<Assignment>, ParseError> {
        let mut assignments = vec![self.parse_assignment()?];
        while self.eat(&TokenKind::Comma) {
            let assignment = self.parse_assignment()?;
            self.check_element_count(assignments.len() + 1, assignment.span)?;
            assignments.push(assignment);
        }
        Ok(assignments)
    }

    fn parse_assignment(&mut self) -> Result<Assignment, ParseError> {
        let path = self.parse_field_path()?;
        self.expect(&TokenKind::Equal, "'='")?;
        let value = self.parse_expression()?;
        Ok(Assignment {
            span: path.span.union(value.span),
            path,
            value,
        })
    }

    fn parse_projections(&mut self) -> Result<ProjectionList, ParseError> {
        if let Some(star) = self.take(&TokenKind::Star) {
            if self.at(&TokenKind::Comma) {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidCombination {
                        what: "'*' cannot be mixed with named projections",
                    },
                    self.peek().span,
                ));
            }
            return Ok(ProjectionList::All(star.span));
        }
        let mut fields = vec![self.parse_projection()?];
        while self.eat(&TokenKind::Comma) {
            if self.at(&TokenKind::Star) {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidCombination {
                        what: "'*' cannot be mixed with named projections",
                    },
                    self.peek().span,
                ));
            }
            let projection = self.parse_projection()?;
            self.check_element_count(fields.len() + 1, projection.span)?;
            fields.push(projection);
        }
        Ok(ProjectionList::Fields(fields))
    }

    fn parse_projection(&mut self) -> Result<Projection, ParseError> {
        let expression = self.parse_expression()?;
        let alias = if self.eat(&TokenKind::As) {
            Some(self.expect_identifier("an alias")?)
        } else {
            None
        };
        if matches!(expression.kind, ExprKind::Traversal(_)) && alias.is_none() {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "graph traversal projections require an AS alias",
                },
                expression.span,
            ));
        }
        let span = alias
            .as_ref()
            .map_or(expression.span, |alias| expression.span.union(alias.span));
        Ok(Projection {
            span,
            expression,
            alias,
        })
    }

    fn parse_order_by(&mut self) -> Result<Vec<OrderBy>, ParseError> {
        let mut terms = Vec::new();
        loop {
            let path = self.parse_field_path()?;
            let direction = if let Some(token) = self.take(&TokenKind::Asc) {
                Spanned::new(OrderDirection::Ascending, token.span)
            } else if let Some(token) = self.take(&TokenKind::Desc) {
                Spanned::new(OrderDirection::Descending, token.span)
            } else {
                Spanned::new(OrderDirection::Ascending, Span::new(path.span.end(), 0))
            };
            let span = path.span.union(direction.span);
            self.check_element_count(terms.len() + 1, span)?;
            terms.push(OrderBy {
                span,
                path,
                direction,
            });
            if !self.eat(&TokenKind::Comma) {
                return Ok(terms);
            }
        }
    }

    fn parse_return_clause(&mut self, context: ReturnContext) -> Result<ReturnClause, ParseError> {
        let return_span = self.tokens[self.position - 1].span;
        let token = self.advance().clone();
        let kind = match token.kind {
            TokenKind::After if context != ReturnContext::Delete => ReturnKind::After,
            TokenKind::None if context != ReturnContext::Delete => ReturnKind::None,
            TokenKind::Before if context != ReturnContext::Update => ReturnKind::Before,
            TokenKind::Eof => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedEof {
                        expected: context.expected_returns(),
                    },
                    token.span,
                ));
            }
            _ => return Err(self.unexpected_at(&token, context.expected_returns())),
        };
        Ok(ReturnClause {
            span: return_span.union(token.span),
            kind: Spanned::new(kind, token.span),
        })
    }

    fn parse_nonnegative_integer(&mut self) -> Result<NonnegativeInteger, ParseError> {
        if self.at(&TokenKind::Minus) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidNumber {
                    literal: "negative integer".into(),
                    reason: "LIMIT and START require a nonnegative integer",
                },
                self.peek().span,
            ));
        }
        let plus = self.take(&TokenKind::Plus).map(|token| token.span);
        let token = self.peek().clone();
        let TokenKind::Number(value) = &token.kind else {
            return Err(self.unexpected("a nonnegative integer"));
        };
        if value.contains(['.', 'e', 'E']) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidNumber {
                    literal: value.clone(),
                    reason: "LIMIT and START do not accept floats or exponents",
                },
                token.span,
            ));
        }
        let parsed = value.parse::<u64>().map_err(|_| {
            ParseError::new(
                ParseErrorKind::InvalidNumber {
                    literal: value.clone(),
                    reason: "integer is outside the supported signed 64-bit range",
                },
                token.span,
            )
        })?;
        if parsed > i64::MAX as u64 {
            return Err(ParseError::new(
                ParseErrorKind::InvalidNumber {
                    literal: value.clone(),
                    reason: "integer is outside the supported signed 64-bit range",
                },
                token.span,
            ));
        }
        self.position += 1;
        let span = plus.map_or(token.span, |plus| plus.union(token.span));
        Ok(NonnegativeInteger {
            value: parsed,
            span,
        })
    }

    fn parse_schema_type(&mut self) -> Result<SchemaType, ParseError> {
        let token = self.advance().clone();
        let kind = match token.kind {
            TokenKind::BoolType => SchemaTypeKind::Bool,
            TokenKind::IntType => SchemaTypeKind::Int,
            TokenKind::FloatType => SchemaTypeKind::Float,
            TokenKind::NumberType => SchemaTypeKind::Number,
            TokenKind::StringType => SchemaTypeKind::String,
            TokenKind::ObjectType => SchemaTypeKind::Object,
            TokenKind::ArrayType if self.at(&TokenKind::Less) => {
                self.advance();
                self.expect(&TokenKind::FloatType, "FLOAT in fixed vector type")?;
                self.expect(&TokenKind::Comma, "',' before vector dimension")?;
                let dimension = self.parse_nonnegative_integer()?;
                if dimension.value == 0 || dimension.value > 65_536 {
                    return Err(ParseError::new(
                        ParseErrorKind::InvalidCombination {
                            what: "fixed vector dimension must be between 1 and 65,536",
                        },
                        dimension.span,
                    ));
                }
                let close = self.expect(&TokenKind::Greater, "'>' after vector dimension")?;
                return Ok(SchemaType {
                    span: token.span.union(close.span),
                    kind: SchemaTypeKind::FixedFloatArray(dimension),
                });
            }
            TokenKind::ArrayType => SchemaTypeKind::Array,
            TokenKind::RecordType => SchemaTypeKind::Record,
            TokenKind::OptionType => {
                self.expect(&TokenKind::Less, "'<' after option")?;
                self.enter_depth(token.span)?;
                let inner_result = self.parse_schema_type();
                self.leave_depth();
                let inner = inner_result?;
                let close = self.expect(&TokenKind::Greater, "'>' after option type")?;
                return Ok(SchemaType {
                    span: token.span.union(close.span),
                    kind: SchemaTypeKind::Option(Box::new(inner)),
                });
            }
            TokenKind::Eof => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedEof {
                        expected: "a schema type",
                    },
                    token.span,
                ));
            }
            _ => return Err(self.unexpected_at(&token, "a schema type")),
        };
        Ok(SchemaType {
            span: token.span,
            kind,
        })
    }

    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_expression_bp(0)
    }

    fn parse_expression_bp(&mut self, minimum_binding_power: u8) -> Result<Expr, ParseError> {
        let mut left = self.parse_prefix_expression()?;
        loop {
            if self.at(&TokenKind::KnnStart) {
                const LEFT_BP: u8 = 5;
                const RIGHT_BP: u8 = 6;
                if LEFT_BP < minimum_binding_power {
                    break;
                }
                let open = self.advance().span;
                let k = self.parse_nonnegative_integer()?;
                if k.value == 0 || k.value > 10_000 {
                    return Err(ParseError::new(
                        ParseErrorKind::InvalidCombination {
                            what: "KNN K must be between 1 and 10,000",
                        },
                        k.span,
                    ));
                }
                self.expect(&TokenKind::Comma, "',' before KNN metric")?;
                let metric_name = self.expect_function_segment("COSINE or EUCLIDEAN")?;
                let metric = if metric_name.value.eq_ignore_ascii_case("cosine") {
                    KnnMetric::Cosine
                } else if metric_name.value.eq_ignore_ascii_case("euclidean") {
                    KnnMetric::Euclidean
                } else {
                    return Err(ParseError::unsupported(
                        "only COSINE and EUCLIDEAN exact KNN metrics are supported",
                        metric_name.span,
                    ));
                };
                let close = self.expect(&TokenKind::KnnEnd, "'|>' after KNN metric")?;
                let query = self.parse_expression_bp(RIGHT_BP)?;
                let span = left.span.union(query.span);
                left = Expr::new(
                    ExprKind::Knn(KnnExpr {
                        field: Box::new(left),
                        k,
                        metric: Spanned::new(metric, metric_name.span),
                        query: Box::new(query),
                        operator_span: open.union(close.span),
                    }),
                    span,
                );
                continue;
            }
            if matches!(
                self.peek().kind,
                TokenKind::ForwardArrow | TokenKind::ReverseArrow | TokenKind::BidirectionalArrow
            ) {
                return Err(ParseError::unsupported(
                    "standalone graph traversal expressions are outside the Phase 7 grammar",
                    self.peek().span,
                ));
            }
            if self.at(&TokenKind::LeftBracket) {
                return Err(ParseError::unsupported(
                    "array indexing is outside the MVP expression grammar",
                    self.peek().span,
                ));
            }
            if let TokenKind::UnsupportedOperator(operator) = self.peek().kind {
                return Err(ParseError::unsupported(
                    excluded_operator_description(operator),
                    self.peek().span,
                ));
            }
            if let TokenKind::Ident(value) = &self.peek().kind {
                if is_excluded_comparison(value) {
                    return Err(ParseError::unsupported(
                        "comparison operator is outside the MVP expression grammar",
                        self.peek().span,
                    ));
                }
            }
            if self.at(&TokenKind::In) {
                return Err(ParseError::unsupported(
                    "comparison operator is outside the MVP expression grammar",
                    self.peek().span,
                ));
            }
            let Some((operator, left_bp, right_bp)) = binary_binding_power(&self.peek().kind)
            else {
                break;
            };
            if left_bp < minimum_binding_power {
                break;
            }
            let token = self.advance().clone();
            let right = self.parse_expression_bp(right_bp)?;
            let span = left.span.union(right.span);
            left = Expr::new(
                ExprKind::Binary {
                    left: Box::new(left),
                    operator: Spanned::new(operator, token.span),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_prefix_expression(&mut self) -> Result<Expr, ParseError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Not | TokenKind::Plus | TokenKind::Minus => {
                self.position += 1;
                if matches!(token.kind, TokenKind::Minus) {
                    if let TokenKind::Number(value) = &self.peek().kind {
                        if !value.contains(['.', 'e', 'E']) && value == "9223372036854775808" {
                            let number_span = self.advance().span;
                            return Ok(Expr::new(
                                ExprKind::Integer(i64::MIN),
                                token.span.union(number_span),
                            ));
                        }
                    }
                }
                self.enter_depth(token.span)?;
                let operand_result = self.parse_expression_bp(13);
                self.leave_depth();
                let operand = operand_result?;
                let operator = match token.kind {
                    TokenKind::Not => UnaryOperator::Not,
                    TokenKind::Plus => UnaryOperator::Plus,
                    TokenKind::Minus => UnaryOperator::Minus,
                    _ => unreachable!(),
                };
                let span = token.span.union(operand.span);
                Ok(Expr::new(
                    ExprKind::Unary {
                        operator: Spanned::new(operator, token.span),
                        operand: Box::new(operand),
                    },
                    span,
                ))
            }
            TokenKind::Null => {
                self.position += 1;
                Ok(Expr::new(ExprKind::Null, token.span))
            }
            TokenKind::True | TokenKind::False => {
                self.position += 1;
                Ok(Expr::new(
                    ExprKind::Bool(matches!(token.kind, TokenKind::True)),
                    token.span,
                ))
            }
            TokenKind::Number(value) => {
                self.position += 1;
                parse_number_expression(value, token.span)
            }
            TokenKind::String(value) => {
                self.position += 1;
                Ok(Expr::new(ExprKind::String(value), token.span))
            }
            TokenKind::Parameter(value) => {
                self.position += 1;
                Ok(Expr::new(ExprKind::Parameter(value), token.span))
            }
            TokenKind::ForwardArrow | TokenKind::ReverseArrow | TokenKind::BidirectionalArrow => {
                self.parse_traversal_expression()
            }
            TokenKind::Ident(_) | TokenKind::Search | TokenKind::In | TokenKind::Out => {
                self.parse_identifier_expression()
            }
            TokenKind::LeftParen => self.parse_parenthesized_expression(),
            TokenKind::LeftBracket => self.parse_array_expression(),
            TokenKind::LeftBrace => self.parse_object_expression(),
            TokenKind::Less => Err(ParseError::unsupported(
                "casts are outside the MVP expression grammar",
                token.span,
            )),
            TokenKind::Select => Err(ParseError::unsupported(
                "subqueries are outside the MVP expression grammar",
                token.span,
            )),
            TokenKind::UnsupportedOperator(operator) => Err(ParseError::unsupported(
                excluded_operator_description(operator),
                token.span,
            )),
            TokenKind::Eof => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: "an expression",
                },
                token.span,
            )),
            _ => Err(self.unexpected("an expression")),
        }
    }

    fn parse_traversal_expression(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek().span;
        let mut hops = Vec::new();
        loop {
            let opening = self.advance().clone();
            let direction = traversal_direction(&opening.kind)
                .ok_or_else(|| self.unexpected_at(&opening, "a graph traversal direction"))?;
            let relation = self.expect_identifier("a relation table after the graph arrow")?;
            let closing = self.advance().clone();
            let closing_direction = traversal_direction(&closing.kind).ok_or_else(|| {
                self.unexpected_at(&closing, "a matching graph arrow after the relation table")
            })?;
            if direction != closing_direction {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidCombination {
                        what: "a graph hop must use the same arrow on both sides of its relation",
                    },
                    opening.span.union(closing.span),
                ));
            }
            let endpoint_table =
                self.expect_identifier("an endpoint table after the graph arrow")?;
            let span = opening.span.union(endpoint_table.span);
            if hops.len() == self.limits.max_graph_hops {
                return Err(ParseError::new(
                    ParseErrorKind::LimitExceeded {
                        kind: LimitKind::GraphHops,
                        limit: self.limits.max_graph_hops,
                    },
                    span,
                ));
            }
            hops.push(TraversalHop {
                span,
                direction: Spanned::new(direction, opening.span.union(closing.span)),
                relation,
                endpoint_table,
            });
            if !matches!(
                self.peek().kind,
                TokenKind::ForwardArrow | TokenKind::ReverseArrow | TokenKind::BidirectionalArrow
            ) {
                break;
            }
        }
        let materialize = if self.eat(&TokenKind::Dot) {
            self.expect(&TokenKind::Star, "'*' after traversal '.'")?;
            true
        } else {
            false
        };
        let end = self.previous_end();
        Ok(Expr::new(
            ExprKind::Traversal(TraversalExpr { hops, materialize }),
            Span::new(start.offset, end - start.offset),
        ))
    }

    fn parse_identifier_expression(&mut self) -> Result<Expr, ParseError> {
        let first = self.expect_function_segment("an identifier")?;
        if self.at(&TokenKind::DoubleColon) || self.at(&TokenKind::LeftParen) {
            return self.parse_function_call(first);
        }
        if self.eat(&TokenKind::Colon) {
            let id = self.parse_record_id_part()?;
            let span = first.span.union(id.span);
            return Ok(Expr::new(
                ExprKind::RecordId(RecordId {
                    span,
                    table: first,
                    id,
                }),
                span,
            ));
        }
        let path = self.parse_field_path_tail(first)?;
        Ok(Expr::new(ExprKind::FieldPath(path.clone()), path.span))
    }

    fn parse_function_call(&mut self, first: Identifier) -> Result<Expr, ParseError> {
        let mut name = vec![first];
        while self.eat(&TokenKind::DoubleColon) {
            let segment = self.expect_function_segment("a function name after '::'")?;
            self.check_element_count(name.len() + 1, segment.span)?;
            name.push(segment);
        }
        self.expect(&TokenKind::LeftParen, "'(' after function name")?;
        self.enter_depth(name[0].span)?;
        let arguments_result = self.parse_function_arguments();
        self.leave_depth();
        let (arguments, close) = arguments_result?;
        let span = name[0].span.union(close);
        Ok(Expr::new(ExprKind::FunctionCall { name, arguments }, span))
    }

    fn parse_function_arguments(&mut self) -> Result<(Vec<Expr>, Span), ParseError> {
        let mut arguments = Vec::new();
        if let Some(close) = self.take(&TokenKind::RightParen) {
            return Ok((arguments, close.span));
        }
        loop {
            let argument = self.parse_expression()?;
            self.check_element_count(arguments.len() + 1, argument.span)?;
            arguments.push(argument);
            if !self.eat(&TokenKind::Comma) {
                let close = self.expect(&TokenKind::RightParen, "')'")?.span;
                return Ok((arguments, close));
            }
            if let Some(close) = self.take(&TokenKind::RightParen) {
                return Ok((arguments, close.span));
            }
        }
    }

    fn parse_parenthesized_expression(&mut self) -> Result<Expr, ParseError> {
        let open = self.expect(&TokenKind::LeftParen, "'('")?.span;
        self.enter_depth(open)?;
        let expression_result = self.parse_expression();
        self.leave_depth();
        let expression = expression_result?;
        let close = self.expect(&TokenKind::RightParen, "')'")?.span;
        Ok(Expr::new(
            ExprKind::Parenthesized(Box::new(expression)),
            open.union(close),
        ))
    }

    fn parse_array_expression(&mut self) -> Result<Expr, ParseError> {
        let open = self.expect(&TokenKind::LeftBracket, "'['")?.span;
        self.enter_depth(open)?;
        let result = self.parse_array_elements();
        self.leave_depth();
        let (elements, close) = result?;
        Ok(Expr::new(ExprKind::Array(elements), open.union(close)))
    }

    fn parse_array_elements(&mut self) -> Result<(Vec<Expr>, Span), ParseError> {
        let mut elements = Vec::new();
        if let Some(close) = self.take(&TokenKind::RightBracket) {
            return Ok((elements, close.span));
        }
        loop {
            let element = self.parse_expression()?;
            self.check_element_count(elements.len() + 1, element.span)?;
            elements.push(element);
            if !self.eat(&TokenKind::Comma) {
                let close = self.expect(&TokenKind::RightBracket, "']'")?.span;
                return Ok((elements, close));
            }
            if let Some(close) = self.take(&TokenKind::RightBracket) {
                return Ok((elements, close.span));
            }
        }
    }

    fn parse_object_expression(&mut self) -> Result<Expr, ParseError> {
        let open = self.expect(&TokenKind::LeftBrace, "'{'")?.span;
        self.enter_depth(open)?;
        let result = self.parse_object_fields();
        self.leave_depth();
        let (fields, close) = result?;
        Ok(Expr::new(ExprKind::Object(fields), open.union(close)))
    }

    fn parse_object_fields(&mut self) -> Result<(Vec<ObjectField>, Span), ParseError> {
        let mut fields = Vec::new();
        if let Some(close) = self.take(&TokenKind::RightBrace) {
            return Ok((fields, close.span));
        }
        loop {
            let key_token = self.advance().clone();
            let key_kind = match key_token.kind {
                TokenKind::Ident(value) => ObjectKeyKind::Identifier(value),
                TokenKind::String(value) => ObjectKeyKind::String(value),
                TokenKind::Eof => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedEof {
                            expected: "an object key",
                        },
                        key_token.span,
                    ));
                }
                _ => return Err(self.unexpected_at(&key_token, "an identifier or string key")),
            };
            self.expect(&TokenKind::Colon, "':' after object key")?;
            let value = self.parse_expression()?;
            let span = key_token.span.union(value.span);
            self.check_element_count(fields.len() + 1, span)?;
            fields.push(ObjectField {
                span,
                key: ObjectKey {
                    span: key_token.span,
                    kind: key_kind,
                },
                value,
            });
            if !self.eat(&TokenKind::Comma) {
                let close = self.expect(&TokenKind::RightBrace, "'}'")?.span;
                return Ok((fields, close));
            }
            if let Some(close) = self.take(&TokenKind::RightBrace) {
                return Ok((fields, close.span));
            }
        }
    }

    fn parse_field_path(&mut self) -> Result<FieldPath, ParseError> {
        let first = self.expect_path_segment("a field path")?;
        self.parse_field_path_tail(first)
    }

    fn parse_field_path_tail(&mut self, first: Identifier) -> Result<FieldPath, ParseError> {
        let start = first.span;
        let mut segments = vec![first];
        while self.eat(&TokenKind::Dot) {
            segments.push(self.expect_path_segment("a path segment after '.'")?);
        }
        let end = segments.last().expect("one path segment").span;
        Ok(FieldPath {
            segments,
            span: start.union(end),
        })
    }

    fn ensure_statement_boundary(&self) -> Result<(), ParseError> {
        if self.at(&TokenKind::Semicolon) || self.at(&TokenKind::Eof) {
            return Ok(());
        }
        if is_unsupported_clause(&self.peek().kind) {
            return Err(ParseError::unsupported(
                "clause is outside the FastDB MVP",
                self.peek().span,
            ));
        }
        if is_statement_start(&self.peek().kind) {
            return Err(ParseError::new(
                ParseErrorKind::MissingStatementSeparator,
                self.peek().span,
            ));
        }
        Err(self.unexpected("a semicolon or end of input"))
    }

    fn check_element_count(&self, count: usize, span: Span) -> Result<(), ParseError> {
        self.check_collection_limit(
            count,
            LimitKind::CollectionElements,
            self.limits.max_collection_elements,
            span,
        )
    }

    fn check_collection_limit(
        &self,
        count: usize,
        kind: LimitKind,
        limit: usize,
        span: Span,
    ) -> Result<(), ParseError> {
        if count <= limit {
            return Ok(());
        }
        Err(ParseError::new(
            ParseErrorKind::LimitExceeded { kind, limit },
            span,
        ))
    }

    fn enter_depth(&mut self, span: Span) -> Result<(), ParseError> {
        if self.depth == self.limits.max_nesting_depth {
            return Err(ParseError::new(
                ParseErrorKind::LimitExceeded {
                    kind: LimitKind::NestingDepth,
                    limit: self.limits.max_nesting_depth,
                },
                span,
            ));
        }
        self.depth += 1;
        Ok(())
    }

    fn leave_depth(&mut self) {
        debug_assert!(self.depth > 0);
        self.depth -= 1;
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn at_offset(&self, offset: usize, expected: &TokenKind) -> bool {
        self.tokens
            .get(self.position + offset)
            .is_some_and(|token| discriminant(&token.kind) == discriminant(expected))
    }

    fn at(&self, expected: &TokenKind) -> bool {
        discriminant(&self.peek().kind) == discriminant(expected)
    }

    fn take(&mut self, expected: &TokenKind) -> Option<Token> {
        if self.at(expected) {
            let token = self.peek().clone();
            self.position += 1;
            Some(token)
        } else {
            None
        }
    }

    fn eat(&mut self, expected: &TokenKind) -> bool {
        self.take(expected).is_some()
    }

    fn expect(
        &mut self,
        expected_kind: &TokenKind,
        expected: &'static str,
    ) -> Result<Token, ParseError> {
        if let Some(token) = self.take(expected_kind) {
            return Ok(token);
        }
        if self.at(&TokenKind::Eof) {
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected },
                self.peek().span,
            ));
        }
        Err(self.unexpected(expected))
    }

    fn expect_identifier(&mut self, expected: &'static str) -> Result<Identifier, ParseError> {
        let token = self.peek().clone();
        if let TokenKind::Ident(value) = token.kind {
            self.position += 1;
            return Ok(Identifier::new(value, token.span));
        }
        if matches!(token.kind, TokenKind::Eof) {
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected },
                token.span,
            ));
        }
        Err(self.unexpected_at(&token, expected))
    }

    fn expect_function_segment(
        &mut self,
        expected: &'static str,
    ) -> Result<Identifier, ParseError> {
        let token = self.peek().clone();
        let value = match token.kind {
            TokenKind::Ident(value) => value,
            TokenKind::Search => "search".to_string(),
            TokenKind::In => "in".to_string(),
            TokenKind::Out => "out".to_string(),
            TokenKind::Eof => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedEof { expected },
                    token.span,
                ));
            }
            _ => return Err(self.unexpected(expected)),
        };
        self.position += 1;
        Ok(Identifier::new(value, token.span))
    }

    fn expect_path_segment(&mut self, expected: &'static str) -> Result<Identifier, ParseError> {
        let token = self.peek().clone();
        let value = match token.kind {
            TokenKind::Ident(value) => value,
            TokenKind::In => "in".to_string(),
            TokenKind::Out => "out".to_string(),
            TokenKind::Eof => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedEof { expected },
                    token.span,
                ));
            }
            _ => return Err(self.unexpected_at(&token, expected)),
        };
        self.position += 1;
        Ok(Identifier::new(value, token.span))
    }

    fn advance(&mut self) -> &Token {
        let index = self.position;
        if !matches!(self.tokens[index].kind, TokenKind::Eof) {
            self.position += 1;
        }
        &self.tokens[index]
    }

    fn previous_end(&self) -> usize {
        self.tokens[self.position.saturating_sub(1)].span.end()
    }

    fn unexpected(&self, expected: &'static str) -> ParseError {
        self.unexpected_at(self.peek(), expected)
    }

    fn unexpected_at(&self, token: &Token, expected: &'static str) -> ParseError {
        if matches!(token.kind, TokenKind::Eof) {
            ParseError::new(ParseErrorKind::UnexpectedEof { expected }, token.span)
        } else {
            ParseError::new(
                ParseErrorKind::UnexpectedToken {
                    expected,
                    found: token.kind.describe(),
                },
                token.span,
            )
        }
    }

    fn duplicate_clause(&self, clause: &'static str) -> ParseError {
        ParseError::new(ParseErrorKind::DuplicateClause { clause }, self.peek().span)
    }

    fn out_of_order_clause(&self, clause: &'static str) -> ParseError {
        ParseError::new(ParseErrorKind::ClauseOrder { clause }, self.peek().span)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReturnContext {
    Create,
    Update,
    Delete,
}

impl ReturnContext {
    const fn expected_returns(self) -> &'static str {
        match self {
            Self::Create => "AFTER, NONE, or BEFORE",
            Self::Update => "AFTER or NONE",
            Self::Delete => "BEFORE",
        }
    }
}

fn parse_number_expression(value: String, span: Span) -> Result<Expr, ParseError> {
    if value.contains(['.', 'e', 'E']) {
        let parsed = value.parse::<f64>().map_err(|_| {
            ParseError::new(
                ParseErrorKind::InvalidNumber {
                    literal: value.clone(),
                    reason: "invalid decimal float",
                },
                span,
            )
        })?;
        if !parsed.is_finite() {
            return Err(ParseError::new(
                ParseErrorKind::InvalidNumber {
                    literal: value,
                    reason: "non-finite floats are not accepted",
                },
                span,
            ));
        }
        Ok(Expr::new(ExprKind::Float(parsed), span))
    } else {
        let parsed = value.parse::<i64>().map_err(|_| {
            ParseError::new(
                ParseErrorKind::InvalidNumber {
                    literal: value,
                    reason: "integer is outside the signed 64-bit range",
                },
                span,
            )
        })?;
        Ok(Expr::new(ExprKind::Integer(parsed), span))
    }
}

fn parse_signed_integer(value: &str, sign: i8, span: Span) -> Result<i64, ParseError> {
    if value.contains(['.', 'e', 'E']) {
        return Err(ParseError::new(
            ParseErrorKind::InvalidNumber {
                literal: value.to_string(),
                reason: "record IDs require an integer without a decimal point or exponent",
            },
            span,
        ));
    }
    let magnitude = value.parse::<u64>().map_err(|_| {
        ParseError::new(
            ParseErrorKind::InvalidNumber {
                literal: value.to_string(),
                reason: "record-ID integer is outside the signed 64-bit range",
            },
            span,
        )
    })?;
    if sign < 0 && magnitude == (i64::MAX as u64) + 1 {
        return Ok(i64::MIN);
    }
    if magnitude > i64::MAX as u64 {
        return Err(ParseError::new(
            ParseErrorKind::InvalidNumber {
                literal: value.to_string(),
                reason: "record-ID integer is outside the signed 64-bit range",
            },
            span,
        ));
    }
    let integer = magnitude as i64;
    Ok(if sign < 0 { -integer } else { integer })
}

fn parse_uuid(value: &str, span: Span) -> Result<uuid::Uuid, ParseError> {
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| {
        ParseError::new(
            ParseErrorKind::InvalidUuid {
                literal: value.to_string(),
                reason: "expected a canonical lowercase hyphenated UUIDv4 or UUIDv7",
            },
            span,
        )
    })?;
    if uuid.hyphenated().to_string() != value {
        return Err(ParseError::new(
            ParseErrorKind::InvalidUuid {
                literal: value.to_string(),
                reason: "UUIDs must use canonical lowercase hyphenated spelling",
            },
            span,
        ));
    }
    if !matches!(uuid.get_version_num(), 4 | 7) {
        return Err(ParseError::new(
            ParseErrorKind::InvalidUuid {
                literal: value.to_string(),
                reason: "only UUIDv4 and UUIDv7 are supported",
            },
            span,
        ));
    }
    Ok(uuid)
}

fn binary_binding_power(kind: &TokenKind) -> Option<(BinaryOperator, u8, u8)> {
    let (operator, power) = match kind {
        TokenKind::Star => (BinaryOperator::Multiply, 11),
        TokenKind::Slash => (BinaryOperator::Divide, 11),
        TokenKind::Plus => (BinaryOperator::Add, 9),
        TokenKind::Minus => (BinaryOperator::Subtract, 9),
        TokenKind::Less => (BinaryOperator::Less, 7),
        TokenKind::LessEqual => (BinaryOperator::LessEqual, 7),
        TokenKind::Greater => (BinaryOperator::Greater, 7),
        TokenKind::GreaterEqual => (BinaryOperator::GreaterEqual, 7),
        TokenKind::Equal => (BinaryOperator::Equal, 5),
        TokenKind::NotEqual => (BinaryOperator::NotEqual, 5),
        TokenKind::FtsMatch(reference) => (BinaryOperator::FtsMatch(*reference), 5),
        TokenKind::And => (BinaryOperator::And, 3),
        TokenKind::Or => (BinaryOperator::Or, 1),
        _ => return None,
    };
    Some((operator, power, power + 1))
}

fn excluded_operator_description(operator: &str) -> &'static str {
    match operator {
        "**" => "power is outside the MVP expression grammar",
        "%" => "modulo is outside the MVP expression grammar",
        "&&" | "||" | "!" => "symbolic boolean operators are outside the MVP grammar",
        "==" => "exact equality is outside the MVP expression grammar",
        ".." => "ranges are outside the MVP expression grammar",
        "->" => "graph traversal is outside the MVP expression grammar",
        _ => "operator is outside the MVP expression grammar",
    }
}

fn is_excluded_comparison(value: &str) -> bool {
    [
        "is",
        "in",
        "inside",
        "contains",
        "containsnot",
        "containsall",
        "containsany",
        "containsnone",
    ]
    .iter()
    .any(|keyword| value.eq_ignore_ascii_case(keyword))
}

fn traversal_direction(kind: &TokenKind) -> Option<TraversalDirection> {
    match kind {
        TokenKind::ForwardArrow => Some(TraversalDirection::Forward),
        TokenKind::ReverseArrow => Some(TraversalDirection::Reverse),
        TokenKind::BidirectionalArrow => Some(TraversalDirection::Bidirectional),
        _ => None,
    }
}

fn is_statement_start(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Create
            | TokenKind::Relate
            | TokenKind::Select
            | TokenKind::Update
            | TokenKind::Delete
            | TokenKind::Define
            | TokenKind::Explain
            | TokenKind::Remove
            | TokenKind::Rebuild
            | TokenKind::Begin
            | TokenKind::Commit
            | TokenKind::Cancel
    ) || is_unsupported_statement(kind)
}

fn is_unsupported_statement(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Insert
            | TokenKind::Upsert
            | TokenKind::Let
            | TokenKind::Info
            | TokenKind::Use
            | TokenKind::Live
            | TokenKind::Show
            | TokenKind::Sleep
            | TokenKind::Throw
            | TokenKind::For
            | TokenKind::If
    )
}

fn is_unsupported_clause(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Timeout
            | TokenKind::Fetch
            | TokenKind::Group
            | TokenKind::Split
            | TokenKind::Omit
            | TokenKind::With
            | TokenKind::Using
            | TokenKind::Value
            | TokenKind::Merge
            | TokenKind::Patch
            | TokenKind::Replace
            | TokenKind::Unset
            | TokenKind::Permissions
            | TokenKind::Assert
            | TokenKind::Default
            | TokenKind::Readonly
            | TokenKind::Changefeed
            | TokenKind::View
            | TokenKind::Fulltext
            | TokenKind::Search
            | TokenKind::Analyzer
            | TokenKind::Parallel
            | TokenKind::Transaction
    )
}
