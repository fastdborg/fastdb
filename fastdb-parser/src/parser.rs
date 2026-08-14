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
        let mut delimiter_depth = 0_usize;
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
            match &token.kind {
                TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => {
                    delimiter_depth = delimiter_depth.saturating_add(1);
                }
                TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                    delimiter_depth = delimiter_depth.saturating_sub(1);
                }
                _ => {}
            }
            let separator = matches!(token.kind, TokenKind::Semicolon) && delimiter_depth == 0;
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

        let mut script = Parser::new(tokens, &self.limits, self.source).parse_script()?;
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
    source: &'a str,
    depth: usize,
    event_action_boundary: bool,
}

impl<'a> Parser<'a> {
    fn new(tokens: Vec<Token>, limits: &'a ParserLimits, source: &'a str) -> Self {
        Self {
            tokens,
            position: 0,
            limits,
            source,
            depth: 0,
            event_action_boundary: false,
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
            TokenKind::Insert => Statement::Insert(self.parse_insert()?),
            TokenKind::Upsert => Statement::Upsert(self.parse_update(true)?),
            TokenKind::Relate => Statement::Relate(self.parse_relate()?),
            TokenKind::Select => Statement::Select(self.parse_select()?),
            TokenKind::Update => Statement::Update(self.parse_update(false)?),
            TokenKind::Delete => Statement::Delete(self.parse_delete()?),
            TokenKind::Let => Statement::Let(self.parse_let()?),
            TokenKind::Return => Statement::ScriptReturn(
                self.parse_script_expression(TokenKind::Return, "keyword RETURN")?,
            ),
            TokenKind::If => Statement::If(self.parse_if()?),
            TokenKind::For => Statement::For(self.parse_for()?),
            TokenKind::Break => Statement::Break(self.parse_control_flow(TokenKind::Break)?),
            TokenKind::Continue => {
                Statement::Continue(self.parse_control_flow(TokenKind::Continue)?)
            }
            TokenKind::Throw => {
                Statement::Throw(self.parse_script_expression(TokenKind::Throw, "keyword THROW")?)
            }
            TokenKind::Sleep => {
                Statement::Sleep(self.parse_script_expression(TokenKind::Sleep, "keyword SLEEP")?)
            }
            TokenKind::Define => self.parse_define()?,
            TokenKind::Alter => self.parse_alter()?,
            TokenKind::Explain => Statement::Explain(self.parse_explain()?),
            TokenKind::Remove => self.parse_remove()?,
            TokenKind::Rebuild => Statement::RebuildIndex(self.parse_index_maintenance(true)?),
            TokenKind::Info => self.parse_info()?,
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

    fn parse_let(&mut self) -> Result<LetStatement, ParseError> {
        let start = self.expect(&TokenKind::Let, "keyword LET")?.span;
        let name = self.expect_parameter("a LET parameter")?;
        self.expect(&TokenKind::Equal, "'=' after LET parameter")?;
        let value = self.parse_expression()?;
        Ok(LetStatement {
            span: start.union(value.span),
            name,
            value,
        })
    }

    fn parse_script_expression(
        &mut self,
        keyword: TokenKind,
        expected: &'static str,
    ) -> Result<ScriptExpressionStatement, ParseError> {
        let start = self.expect(&keyword, expected)?.span;
        let value = self.parse_expression()?;
        Ok(ScriptExpressionStatement {
            span: start.union(value.span),
            value,
        })
    }

    fn parse_if(&mut self) -> Result<IfStatement, ParseError> {
        let start = self.expect(&TokenKind::If, "keyword IF")?.span;
        let mut branches = Vec::new();
        let condition = self.parse_expression()?;
        let body = self.parse_script_block()?;
        branches.push((condition, body));
        let mut otherwise = None;
        while self.eat(&TokenKind::Else) {
            if self.eat(&TokenKind::If) {
                let condition = self.parse_expression()?;
                let body = self.parse_script_block()?;
                self.check_element_count(branches.len() + 1, body.span)?;
                branches.push((condition, body));
            } else {
                otherwise = Some(self.parse_script_block()?);
                break;
            }
        }
        let end = otherwise.as_ref().map_or_else(
            || branches.last().expect("one IF branch").1.span,
            |block| block.span,
        );
        Ok(IfStatement {
            span: start.union(end),
            branches,
            otherwise,
        })
    }

    fn parse_for(&mut self) -> Result<ForStatement, ParseError> {
        let start = self.expect(&TokenKind::For, "keyword FOR")?.span;
        let binding = self.expect_parameter("a FOR binding parameter")?;
        self.expect(&TokenKind::In, "keyword IN after FOR binding")?;
        let iterable = self.parse_expression()?;
        let body = self.parse_script_block()?;
        Ok(ForStatement {
            span: start.union(body.span),
            binding,
            iterable,
            body,
        })
    }

    fn parse_control_flow(
        &mut self,
        keyword: TokenKind,
    ) -> Result<ControlFlowStatement, ParseError> {
        let token = self.expect(&keyword, "control-flow keyword")?;
        Ok(ControlFlowStatement { span: token.span })
    }

    fn parse_script_block(&mut self) -> Result<ScriptBlock, ParseError> {
        let open = self.expect(&TokenKind::LeftBrace, "'{' before script block")?;
        self.with_depth(|parser| {
            let mut statements = Vec::new();
            if parser.at(&TokenKind::RightBrace) {
                let close = parser.advance().clone();
                return Ok(ScriptBlock {
                    span: open.span.union(close.span),
                    statements,
                });
            }
            loop {
                let statement = parser.parse_statement()?;
                parser.check_collection_limit(
                    statements.len() + 1,
                    LimitKind::Statements,
                    parser.limits.max_statements,
                    statement.span(),
                )?;
                statements.push(statement);
                if parser.eat(&TokenKind::Semicolon) {
                    if parser.at(&TokenKind::RightBrace) {
                        let close = parser.advance().clone();
                        return Ok(ScriptBlock {
                            span: open.span.union(close.span),
                            statements,
                        });
                    }
                    continue;
                }
                let close = parser.expect(&TokenKind::RightBrace, "'}' after script block")?;
                return Ok(ScriptBlock {
                    span: open.span.union(close.span),
                    statements,
                });
            }
        })
    }

    fn parse_create(&mut self) -> Result<CreateStatement, ParseError> {
        let start = self.expect(&TokenKind::Create, "keyword CREATE")?.span;
        let only = self.take(&TokenKind::Only).map(|token| token.span);
        let target = if let Some(open) = self.take(&TokenKind::Pipe) {
            let target = self.parse_target()?;
            if !matches!(target, Target::Record(_) | Target::RecordRange(_)) {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidCombination {
                        what: "batch CREATE requires a count or integer record range",
                    },
                    target.span(),
                ));
            }
            let close = self.expect(&TokenKind::Pipe, "'|' after batch CREATE target")?;
            Target::Batch {
                span: open.span.union(close.span),
                target: Box::new(target),
            }
        } else {
            self.parse_mutation_target()?
        };
        let data = if self.eat(&TokenKind::Content) {
            Some(CreateData::Content(self.parse_expression()?))
        } else if self.eat(&TokenKind::Set) {
            Some(CreateData::Set(self.parse_assignments()?))
        } else {
            None
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
        if self.at(&TokenKind::Version) {
            return Err(ParseError::unsupported(
                "CREATE VERSION requires versioned history, which is excluded from the pre-1.0 roadmap",
                self.peek().span,
            ));
        }
        let timeout = self.parse_timeout_clause()?;
        if self.at(&TokenKind::Version) {
            return Err(ParseError::unsupported(
                "CREATE VERSION requires versioned history, which is excluded from the pre-1.0 roadmap",
                self.peek().span,
            ));
        }
        let end = self.previous_end();
        Ok(CreateStatement {
            span: Span::new(start.offset, end - start.offset),
            only,
            target,
            data,
            return_clause,
            timeout,
        })
    }

    fn parse_insert(&mut self) -> Result<InsertStatement, ParseError> {
        let start = self.expect(&TokenKind::Insert, "keyword INSERT")?.span;
        let relation = self.take(&TokenKind::Relation).map(|token| token.span);
        let ignore = self.take(&TokenKind::Ignore).map(|token| token.span);
        self.expect(&TokenKind::Into, "keyword INTO")?;
        let table = self.expect_identifier("a table name")?;
        let data = if self.eat(&TokenKind::LeftParen) {
            let mut fields = vec![self.expect_identifier("an inserted field")?];
            while self.eat(&TokenKind::Comma) {
                let field = self.expect_identifier("an inserted field")?;
                self.check_element_count(fields.len() + 1, field.span)?;
                fields.push(field);
            }
            self.expect(&TokenKind::RightParen, "')' after inserted fields")?;
            self.expect(&TokenKind::Values, "keyword VALUES")?;
            let mut rows = Vec::new();
            loop {
                let open = self.expect(&TokenKind::LeftParen, "'(' before inserted values")?;
                let mut values = vec![self.parse_expression()?];
                while self.eat(&TokenKind::Comma) {
                    let value = self.parse_expression()?;
                    self.check_element_count(values.len() + 1, value.span)?;
                    values.push(value);
                }
                let close = self.expect(&TokenKind::RightParen, "')' after inserted values")?;
                if values.len() != fields.len() {
                    return Err(ParseError::new(
                        ParseErrorKind::InvalidCombination {
                            what: "each INSERT VALUES row must match the field count",
                        },
                        open.span.union(close.span),
                    ));
                }
                self.check_element_count(rows.len() + 1, close.span)?;
                rows.push(values);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            InsertData::Values { fields, rows }
        } else {
            InsertData::Expression(self.parse_expression()?)
        };
        let on_duplicate = if self.eat(&TokenKind::On) {
            self.expect(&TokenKind::Duplicate, "keyword DUPLICATE")?;
            self.expect(&TokenKind::Key, "keyword KEY")?;
            self.expect(&TokenKind::Update, "keyword UPDATE")?;
            self.parse_assignments()?
        } else {
            Vec::new()
        };
        let return_clause = if self.eat(&TokenKind::Return) {
            Some(self.parse_return_clause(ReturnContext::Create)?)
        } else {
            None
        };
        let timeout = self.parse_timeout_clause()?;
        let end = self.previous_end();
        Ok(InsertStatement {
            span: Span::new(start.offset, end - start.offset),
            relation,
            ignore,
            table,
            data,
            on_duplicate,
            return_clause,
            timeout,
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
        let value = self.take(&TokenKind::Value).map(|token| token.span);
        let (projections, include_all) = self.parse_projections()?;
        let omit = if self.eat(&TokenKind::Omit) {
            self.parse_field_paths()?
        } else {
            Vec::new()
        };
        self.expect(&TokenKind::From, "keyword FROM")?;
        let only = self.take(&TokenKind::Only).map(|token| token.span);
        let target = self.parse_select_target()?;
        let mut additional_targets = Vec::new();
        while self.eat(&TokenKind::Comma) {
            let target = self.parse_select_target()?;
            self.check_element_count(additional_targets.len() + 2, select_target_span(&target))?;
            additional_targets.push(target);
        }
        let condition = if self.eat(&TokenKind::Where) {
            Some(self.parse_expression()?)
        } else {
            None
        };
        let split = if self.eat(&TokenKind::Split) {
            self.expect(&TokenKind::On, "keyword ON after SPLIT")?;
            self.parse_field_paths()?
        } else {
            Vec::new()
        };
        let group = if self.eat(&TokenKind::Group) {
            if let Some(all) = self.take(&TokenKind::All) {
                Some(GroupClause::All(all.span))
            } else {
                self.expect(&TokenKind::By, "keyword BY after GROUP")?;
                let mut expressions = vec![self.parse_expression()?];
                while self.eat(&TokenKind::Comma) {
                    let expression = self.parse_expression()?;
                    self.check_element_count(expressions.len() + 1, expression.span)?;
                    expressions.push(expression);
                }
                Some(GroupClause::By(expressions))
            }
        } else {
            None
        };
        let (order_by, order_random) = if self.eat(&TokenKind::Order) {
            self.expect(&TokenKind::By, "keyword BY after ORDER")?;
            if self.at(&TokenKind::Rand)
                || matches!(&self.peek().kind, TokenKind::Ident(name) if name.eq_ignore_ascii_case("rand"))
            {
                let rand = self.advance().span;
                self.expect(&TokenKind::LeftParen, "'(' after RAND")?;
                let close = self.expect(&TokenKind::RightParen, "')' after RAND")?.span;
                (Vec::new(), Some(rand.union(close)))
            } else {
                (self.parse_order_by()?, None)
            }
        } else {
            (Vec::new(), None)
        };
        let (limit, limit_expression) = if self.eat(&TokenKind::Limit) {
            self.eat(&TokenKind::By);
            if matches!(
                self.peek().kind,
                TokenKind::Number(_) | TokenKind::Plus | TokenKind::Minus
            ) {
                (Some(self.parse_nonnegative_integer()?), None)
            } else {
                (None, Some(self.parse_expression()?))
            }
        } else {
            (None, None)
        };
        let (start_value, start_expression) = if self.eat(&TokenKind::Start) {
            self.eat(&TokenKind::At);
            if matches!(
                self.peek().kind,
                TokenKind::Number(_) | TokenKind::Plus | TokenKind::Minus
            ) {
                (Some(self.parse_nonnegative_integer()?), None)
            } else {
                (None, Some(self.parse_expression()?))
            }
        } else {
            (None, None)
        };
        let fetch = if self.eat(&TokenKind::Fetch) {
            self.parse_field_paths()?
        } else {
            Vec::new()
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
            value,
            projections,
            include_all,
            only,
            target,
            additional_targets,
            condition,
            split,
            group,
            omit,
            order_by,
            order_random,
            limit,
            limit_expression,
            start: start_value,
            start_expression,
            fetch,
        })
    }

    fn parse_select_target(&mut self) -> Result<SelectTarget, ParseError> {
        if self.eat(&TokenKind::LeftParen) {
            if !self.at(&TokenKind::Select) {
                return Err(self.unexpected("a SELECT subquery"));
            }
            let select = self.parse_select()?;
            self.expect(&TokenKind::RightParen, "')' after SELECT subquery")?;
            return Ok(SelectTarget::Subquery(Box::new(select)));
        }
        if matches!(
            self.peek().kind,
            TokenKind::LeftBracket
                | TokenKind::LeftBrace
                | TokenKind::Parameter(_)
                | TokenKind::String(_)
                | TokenKind::Number(_)
                | TokenKind::Null
                | TokenKind::None
                | TokenKind::True
                | TokenKind::False
        ) {
            return self.parse_expression().map(SelectTarget::Expression);
        }
        self.parse_target().map(SelectTarget::Target)
    }

    fn parse_field_paths(&mut self) -> Result<Vec<FieldPath>, ParseError> {
        let mut paths = vec![self.parse_field_path()?];
        while self.eat(&TokenKind::Comma) {
            let path = self.parse_field_path()?;
            self.check_element_count(paths.len() + 1, path.span)?;
            paths.push(path);
        }
        Ok(paths)
    }

    fn parse_update(&mut self, upsert: bool) -> Result<UpdateStatement, ParseError> {
        let keyword = if upsert {
            TokenKind::Upsert
        } else {
            TokenKind::Update
        };
        let start = self.expect(&keyword, "UPDATE or UPSERT")?.span;
        let only = self.take(&TokenKind::Only).map(|token| token.span);
        let target = self.parse_mutation_target()?;
        let data = self.parse_update_data()?;
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
        let timeout = self.parse_timeout_clause()?;
        let end = self.previous_end();
        Ok(UpdateStatement {
            span: Span::new(start.offset, end - start.offset),
            only,
            target,
            data,
            condition,
            return_clause,
            timeout,
        })
    }

    fn parse_update_data(&mut self) -> Result<UpdateData, ParseError> {
        if self.eat(&TokenKind::Content) {
            Ok(UpdateData::Content(self.parse_expression()?))
        } else if self.eat(&TokenKind::Merge) {
            Ok(UpdateData::Merge(self.parse_expression()?))
        } else if self.eat(&TokenKind::Patch) {
            Ok(UpdateData::Patch(self.parse_expression()?))
        } else if self.eat(&TokenKind::Replace) {
            Ok(UpdateData::Replace(self.parse_expression()?))
        } else if self.eat(&TokenKind::Set) {
            Ok(UpdateData::Set(self.parse_assignments()?))
        } else if self.eat(&TokenKind::Unset) {
            let mut paths = vec![self.parse_field_path()?];
            while self.eat(&TokenKind::Comma) {
                let path = self.parse_field_path()?;
                self.check_element_count(paths.len() + 1, path.span)?;
                paths.push(path);
            }
            Ok(UpdateData::Unset(paths))
        } else {
            Err(self.unexpected("CONTENT, MERGE, PATCH, REPLACE, SET, or UNSET"))
        }
    }

    fn parse_delete(&mut self) -> Result<DeleteStatement, ParseError> {
        let start = self.expect(&TokenKind::Delete, "keyword DELETE")?.span;
        self.eat(&TokenKind::From);
        let only = self.take(&TokenKind::Only).map(|token| token.span);
        let target = self.parse_mutation_target()?;
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
        let timeout = self.parse_timeout_clause()?;
        let end = self.previous_end();
        Ok(DeleteStatement {
            span: Span::new(start.offset, end - start.offset),
            only,
            target,
            condition,
            return_clause,
            timeout,
        })
    }

    fn parse_timeout_clause(&mut self) -> Result<Option<Expr>, ParseError> {
        if !self.eat(&TokenKind::Timeout) {
            return Ok(None);
        }
        let timeout = self.parse_expression()?;
        if !matches!(timeout.kind, ExprKind::Duration(_) | ExprKind::Parameter(_)) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "TIMEOUT requires a duration literal or bound duration parameter",
                },
                timeout.span,
            ));
        }
        if self.at(&TokenKind::Timeout) {
            return Err(self.duplicate_clause("TIMEOUT"));
        }
        Ok(Some(timeout))
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
            TokenKind::Param => Ok(Statement::DefineParam(self.parse_define_param(start)?)),
            TokenKind::Function => Ok(Statement::DefineFunction(
                self.parse_define_function(start)?,
            )),
            TokenKind::Ident(value) if value.eq_ignore_ascii_case("event") => Ok(
                Statement::DefineEvent(self.parse_define_event(start)?),
            ),
            TokenKind::Ident(value) if value.eq_ignore_ascii_case("sequence") => {
                Err(ParseError::unsupported(
                    "SurrealDB sequence allocation cannot preserve its non-rollback semantics on serialized stable WAL",
                    self.peek().span,
                ))
            }
            TokenKind::Ident(value) if value.eq_ignore_ascii_case("module") => {
                Err(ParseError::unsupported(
                    "DEFINE MODULE requires an unavailable sealed bounded WASM provider",
                    self.peek().span,
                ))
            }
            TokenKind::Ident(value) if value.eq_ignore_ascii_case("api") => {
                Err(ParseError::unsupported(
                    "DEFINE API execution belongs to the authenticated Phase 19 server",
                    self.peek().span,
                ))
            }
            _ => Err(ParseError::unsupported(
                "this DEFINE target is outside the active FastDB grammar",
                self.peek().span,
            )),
        }
    }

    fn parse_define_param(&mut self, start: Span) -> Result<DefineParamStatement, ParseError> {
        self.expect(&TokenKind::Param, "keyword PARAM")?;
        let if_not_exists = if let Some(if_token) = self.take(&TokenKind::If) {
            self.expect(&TokenKind::Not, "keyword NOT after IF")?;
            let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF NOT")?;
            Some(if_token.span.union(exists.span))
        } else {
            None
        };
        let overwrite = self.take(&TokenKind::Overwrite).map(|token| token.span);
        if let (Some(_), Some(overwrite_span)) = (if_not_exists, overwrite) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "DEFINE PARAM cannot combine IF NOT EXISTS and OVERWRITE",
                },
                overwrite_span,
            ));
        }
        let name = self.expect_parameter("a database parameter")?;
        self.expect(&TokenKind::Value, "keyword VALUE")?;
        let value = self.parse_expression()?;
        let permissions = self
            .parse_schema_permissions()?
            .unwrap_or(SchemaPermissions::Full);
        self.validate_simple_permissions(&permissions, "parameter")?;
        Ok(DefineParamStatement {
            span: Span::new(start.offset, self.previous_end() - start.offset),
            if_not_exists,
            overwrite,
            name,
            value,
            permissions,
        })
    }

    fn parse_define_function(
        &mut self,
        start: Span,
    ) -> Result<DefineFunctionStatement, ParseError> {
        self.expect(&TokenKind::Function, "keyword FUNCTION")?;
        let if_not_exists = if let Some(if_token) = self.take(&TokenKind::If) {
            self.expect(&TokenKind::Not, "keyword NOT after IF")?;
            let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF NOT")?;
            Some(if_token.span.union(exists.span))
        } else {
            None
        };
        let overwrite = self.take(&TokenKind::Overwrite).map(|token| token.span);
        if let (Some(_), Some(overwrite_span)) = (if_not_exists, overwrite) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "DEFINE FUNCTION cannot combine IF NOT EXISTS and OVERWRITE",
                },
                overwrite_span,
            ));
        }
        let name = self.parse_custom_function_name()?;
        self.expect(&TokenKind::LeftParen, "'(' after function name")?;
        let mut arguments = Vec::new();
        if !self.eat(&TokenKind::RightParen) {
            loop {
                let name = self.expect_parameter("a typed function argument")?;
                if arguments
                    .iter()
                    .any(|argument: &FunctionArgument| argument.name.value == name.value)
                {
                    return Err(ParseError::new(
                        ParseErrorKind::InvalidCombination {
                            what: "duplicate function argument",
                        },
                        name.span,
                    ));
                }
                self.expect(&TokenKind::Colon, "':' after function argument")?;
                let ty = self.parse_schema_type()?;
                let span = name.span.union(ty.span);
                arguments.push(FunctionArgument { span, name, ty });
                self.check_element_count(arguments.len(), span)?;
                if !self.eat(&TokenKind::Comma) {
                    self.expect(&TokenKind::RightParen, "')' after function arguments")?;
                    break;
                }
            }
        }
        let body = self.parse_script_block()?;
        let permissions = self
            .parse_schema_permissions()?
            .unwrap_or(SchemaPermissions::Full);
        self.validate_simple_permissions(&permissions, "function")?;
        Ok(DefineFunctionStatement {
            span: Span::new(start.offset, self.previous_end() - start.offset),
            if_not_exists,
            overwrite,
            name,
            arguments,
            body,
            permissions,
        })
    }

    fn parse_custom_function_name(&mut self) -> Result<Vec<Identifier>, ParseError> {
        let first = self.expect_function_segment("function namespace fn")?;
        if !first.value.eq_ignore_ascii_case("fn") {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "custom function names must begin with fn::",
                },
                first.span,
            ));
        }
        self.expect(&TokenKind::DoubleColon, "'::' after fn")?;
        let mut name = vec![first];
        loop {
            let segment = self.expect_function_segment("a custom function name segment")?;
            self.check_element_count(name.len() + 1, segment.span)?;
            name.push(segment);
            if !self.eat(&TokenKind::DoubleColon) {
                break;
            }
        }
        Ok(name)
    }

    fn parse_define_event(&mut self, start: Span) -> Result<DefineEventStatement, ParseError> {
        self.expect_ident_keyword("event", "keyword EVENT")?;
        let if_not_exists = if let Some(if_token) = self.take(&TokenKind::If) {
            self.expect(&TokenKind::Not, "keyword NOT after IF")?;
            let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF NOT")?;
            Some(if_token.span.union(exists.span))
        } else {
            None
        };
        let overwrite = self.take(&TokenKind::Overwrite).map(|token| token.span);
        if let (Some(_), Some(overwrite_span)) = (if_not_exists, overwrite) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "DEFINE EVENT cannot combine IF NOT EXISTS and OVERWRITE",
                },
                overwrite_span,
            ));
        }
        let name = self.expect_identifier("an event name")?;
        self.expect(&TokenKind::On, "keyword ON")?;
        let table_keyword = self.take(&TokenKind::Table).map(|token| token.span);
        let table = self.expect_identifier("an event table")?;
        let mut condition = None;
        let mut action = None;
        let mut comment = None;
        loop {
            if self.eat_ident_keyword("async") {
                return Err(ParseError::unsupported(
                    "asynchronous events are outside the synchronous Phase 15 event contract",
                    self.previous_span(),
                ));
            }
            if self.eat_ident_keyword("when") {
                if condition.is_some() {
                    return Err(self.duplicate_clause("WHEN"));
                }
                condition = Some(self.parse_expression()?);
                continue;
            }
            if self.eat_ident_keyword("then") {
                if action.is_some() {
                    return Err(self.duplicate_clause("THEN"));
                }
                action = Some(self.parse_event_action()?);
                continue;
            }
            if self.at(&TokenKind::Comment) {
                if comment.is_some() {
                    return Err(self.duplicate_clause("COMMENT"));
                }
                comment = self.parse_optional_comment()?;
                continue;
            }
            break;
        }
        let action = action.ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "DEFINE EVENT requires at least one THEN action",
                },
                table.span,
            )
        })?;
        Ok(DefineEventStatement {
            span: Span::new(start.offset, self.previous_end() - start.offset),
            if_not_exists,
            overwrite,
            name,
            table_keyword,
            table,
            condition,
            action,
            comment,
        })
    }

    fn parse_event_action(&mut self) -> Result<EventAction, ParseError> {
        if self.at(&TokenKind::LeftBrace) {
            let block = self.parse_script_block()?;
            return Ok(EventAction {
                span: block.span,
                block,
                style: EventActionStyle::Block,
            });
        }
        if self.at(&TokenKind::LeftParen) {
            let block = self.parse_parenthesized_script_block()?;
            return Ok(EventAction {
                span: block.span,
                block,
                style: EventActionStyle::Parenthesized,
            });
        }
        self.event_action_boundary = true;
        let statement = self.parse_statement();
        self.event_action_boundary = false;
        let statement = statement?;
        let span = statement.span();
        Ok(EventAction {
            span,
            block: ScriptBlock {
                span,
                statements: vec![statement],
            },
            style: EventActionStyle::Bare,
        })
    }

    fn parse_parenthesized_script_block(&mut self) -> Result<ScriptBlock, ParseError> {
        let open = self.expect(&TokenKind::LeftParen, "'(' before event action")?;
        self.with_depth(|parser| {
            if parser.at(&TokenKind::RightParen) {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidCombination {
                        what: "event THEN action cannot be empty",
                    },
                    parser.peek().span,
                ));
            }
            let mut statements = Vec::new();
            loop {
                parser.event_action_boundary = true;
                let statement = parser.parse_statement();
                parser.event_action_boundary = false;
                let statement = statement?;
                parser.check_collection_limit(
                    statements.len() + 1,
                    LimitKind::Statements,
                    parser.limits.max_statements,
                    statement.span(),
                )?;
                statements.push(statement);
                if parser.eat(&TokenKind::Semicolon) || parser.eat(&TokenKind::Comma) {
                    if parser.at(&TokenKind::RightParen) {
                        let close = parser.advance().clone();
                        return Ok(ScriptBlock {
                            span: open.span.union(close.span),
                            statements,
                        });
                    }
                    continue;
                }
                let close = parser.expect(&TokenKind::RightParen, "')' after event action")?;
                return Ok(ScriptBlock {
                    span: open.span.union(close.span),
                    statements,
                });
            }
        })
    }

    fn parse_schema_permissions(&mut self) -> Result<Option<SchemaPermissions>, ParseError> {
        if !self.eat(&TokenKind::Permissions) {
            return Ok(None);
        }
        if self.eat(&TokenKind::Full) {
            Ok(Some(SchemaPermissions::Full))
        } else if self.eat(&TokenKind::None) {
            Ok(Some(SchemaPermissions::None))
        } else {
            let mut clauses = Vec::new();
            let mut seen = Vec::new();
            loop {
                let start = self.expect(&TokenKind::For, "FULL, NONE, or FOR after PERMISSIONS")?;
                let mut actions = vec![self.parse_schema_permission_action()?];
                while self.at(&TokenKind::Comma) && !self.at_offset(1, &TokenKind::For) {
                    self.advance();
                    actions.push(self.parse_schema_permission_action()?);
                    self.check_element_count(actions.len(), start.span)?;
                }
                for action in &actions {
                    if seen.contains(action) {
                        return Err(ParseError::new(
                            ParseErrorKind::InvalidCombination {
                                what: "duplicate permission action",
                            },
                            start.span,
                        ));
                    }
                    seen.push(*action);
                }
                let value = if self.eat(&TokenKind::Full) {
                    SchemaPermissionValue::Full
                } else if self.eat(&TokenKind::None) {
                    SchemaPermissionValue::None
                } else {
                    self.expect(
                        &TokenKind::Where,
                        "FULL, NONE, or WHERE after permission actions",
                    )?;
                    let expression = self.parse_expression()?;
                    let source = self
                        .source
                        .get(expression.span.offset..expression.span.end())
                        .ok_or_else(|| {
                            ParseError::new(
                                ParseErrorKind::InvalidCombination {
                                    what: "permission expression span is outside the source",
                                },
                                expression.span,
                            )
                        })?
                        .to_string();
                    SchemaPermissionValue::Where { expression, source }
                };
                let end = self.previous_end();
                clauses.push(SchemaPermissionClause {
                    span: Span::new(start.span.offset, end - start.span.offset),
                    actions,
                    value,
                });
                self.check_element_count(clauses.len(), start.span)?;
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
                if !self.at(&TokenKind::For) {
                    return Err(self.unexpected("FOR after permission clause comma"));
                }
            }
            Ok(Some(SchemaPermissions::Specific(clauses)))
        }
    }

    fn parse_schema_permission_action(&mut self) -> Result<SchemaPermissionAction, ParseError> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Select => Ok(SchemaPermissionAction::Select),
            TokenKind::Create => Ok(SchemaPermissionAction::Create),
            TokenKind::Update => Ok(SchemaPermissionAction::Update),
            TokenKind::Delete => Ok(SchemaPermissionAction::Delete),
            _ => Err(self.unexpected_at(&token, "select, create, update, or delete")),
        }
    }

    fn validate_simple_permissions(
        &self,
        permissions: &SchemaPermissions,
        target: &'static str,
    ) -> Result<(), ParseError> {
        if matches!(permissions, SchemaPermissions::Specific(_)) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: match target {
                        "parameter" => "parameter permissions accept only FULL or NONE",
                        "function" => "function permissions accept only FULL or NONE",
                        _ => "this schema target accepts only FULL or NONE permissions",
                    },
                },
                self.peek().span,
            ));
        }
        Ok(())
    }

    fn validate_field_permissions(
        &self,
        permissions: &SchemaPermissions,
    ) -> Result<(), ParseError> {
        if let SchemaPermissions::Specific(clauses) = permissions {
            if clauses
                .iter()
                .any(|clause| clause.actions.contains(&SchemaPermissionAction::Delete))
            {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidCombination {
                        what: "field permissions do not accept delete actions",
                    },
                    clauses
                        .iter()
                        .find(|clause| clause.actions.contains(&SchemaPermissionAction::Delete))
                        .map_or(self.peek().span, |clause| clause.span),
                ));
            }
        }
        Ok(())
    }

    fn parse_reference_delete_action(
        &mut self,
    ) -> Result<Option<ReferenceDeleteAction>, ParseError> {
        if !self.eat(&TokenKind::On) {
            return Ok(None);
        }
        self.expect(&TokenKind::Delete, "keyword DELETE after REFERENCE ON")?;
        let token = self.advance().clone();
        let action = match token.kind {
            TokenKind::Ident(value) if value.eq_ignore_ascii_case("cascade") => {
                ReferenceDeleteAction::Cascade
            }
            TokenKind::Ident(value) if value.eq_ignore_ascii_case("reject") => {
                ReferenceDeleteAction::Reject
            }
            TokenKind::Unset => ReferenceDeleteAction::Unset,
            TokenKind::Ignore => ReferenceDeleteAction::Ignore,
            _ => {
                return Err(self.unexpected_at(
                    &token,
                    "CASCADE, REJECT, UNSET, or IGNORE after REFERENCE ON DELETE",
                ))
            }
        };
        Ok(Some(action))
    }

    fn parse_alter(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(&TokenKind::Alter, "keyword ALTER")?.span;
        if matches!(&self.peek().kind, TokenKind::Ident(value) if value.eq_ignore_ascii_case("sequence"))
        {
            return Err(ParseError::unsupported(
                "SurrealDB sequence allocation cannot preserve its non-rollback semantics on serialized stable WAL",
                self.peek().span,
            ));
        }
        if matches!(&self.peek().kind, TokenKind::Ident(value) if value.eq_ignore_ascii_case("api"))
        {
            return Err(ParseError::unsupported(
                "ALTER API execution belongs to the authenticated Phase 19 server",
                self.peek().span,
            ));
        }
        if self.at_ident_keyword("event") {
            return self.parse_alter_event(start).map(Statement::AlterEvent);
        }
        if self.eat(&TokenKind::Field) {
            let if_exists = if let Some(if_token) = self.take(&TokenKind::If) {
                let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF")?;
                Some(if_token.span.union(exists.span))
            } else {
                None
            };
            let path = self.parse_field_path()?;
            self.expect(&TokenKind::On, "keyword ON")?;
            let table_keyword = self.take(&TokenKind::Table).map(|token| token.span);
            let table = self.expect_identifier("a table name")?;
            let change = if self.eat(&TokenKind::Drop) {
                if self.eat(&TokenKind::Type) {
                    AlterFieldChange::DropType
                } else if self.eat_ident_keyword("flexible") {
                    AlterFieldChange::DropFlexible
                } else if self.eat(&TokenKind::Default) {
                    AlterFieldChange::DropDefault
                } else if self.eat(&TokenKind::Value) {
                    AlterFieldChange::DropValue
                } else if self.eat(&TokenKind::Assert) {
                    AlterFieldChange::DropAssert
                } else if self.eat(&TokenKind::Readonly) {
                    AlterFieldChange::DropReadonly
                } else if self.eat(&TokenKind::Reference) {
                    AlterFieldChange::DropReference
                } else if self.eat(&TokenKind::Comment) {
                    AlterFieldChange::DropComment
                } else {
                    return Err(self.unexpected("a supported field clause after DROP"));
                }
            } else if self.eat(&TokenKind::Type) {
                AlterFieldChange::Type(self.parse_schema_type()?)
            } else if self.eat_ident_keyword("flexible") {
                AlterFieldChange::Flexible
            } else if let Some(token) = self.take(&TokenKind::Default) {
                let always = self.take(&TokenKind::Always).map(|token| token.span);
                let value = self.parse_expression()?;
                AlterFieldChange::Default(FieldDefaultClause {
                    span: token.span.union(value.span),
                    always,
                    value,
                })
            } else if self.eat(&TokenKind::Value) {
                AlterFieldChange::Value(self.parse_expression()?)
            } else if self.eat(&TokenKind::Assert) {
                AlterFieldChange::Assert(self.parse_expression()?)
            } else if self.eat(&TokenKind::Readonly) {
                AlterFieldChange::Readonly
            } else if self.eat(&TokenKind::Reference) {
                AlterFieldChange::Reference(self.parse_reference_delete_action()?)
            } else if self.at(&TokenKind::Permissions) {
                let permissions = self
                    .parse_schema_permissions()?
                    .expect("PERMISSIONS was present");
                self.validate_field_permissions(&permissions)?;
                AlterFieldChange::Permissions(permissions)
            } else if self.at(&TokenKind::Comment) {
                AlterFieldChange::Comment(
                    self.parse_optional_comment()?
                        .expect("COMMENT was present")
                        .value,
                )
            } else {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidCombination {
                        what: "ALTER FIELD requires one supported clause",
                    },
                    table.span,
                ));
            };
            return Ok(Statement::AlterField(AlterFieldStatement {
                span: Span::new(start.offset, self.previous_end() - start.offset),
                if_exists,
                path,
                table_keyword,
                table,
                change,
            }));
        }
        if self.eat(&TokenKind::Table) {
            let if_exists = if let Some(if_token) = self.take(&TokenKind::If) {
                let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF")?;
                Some(if_token.span.union(exists.span))
            } else {
                None
            };
            let name = self.expect_identifier("a table name")?;
            let mut mode = None;
            let mut permissions = None;
            let mut comment = TableCommentChange::Unchanged;
            if let Some(token) = self.take(&TokenKind::Schemafull) {
                mode = Some(Spanned::new(TableMode::Schemafull, token.span));
            } else if let Some(token) = self.take(&TokenKind::Schemaless) {
                mode = Some(Spanned::new(TableMode::Schemaless, token.span));
            } else if self.eat(&TokenKind::Drop) {
                if self.eat(&TokenKind::Comment) {
                    comment = TableCommentChange::Drop;
                } else if self.at(&TokenKind::Changefeed) {
                    return Err(ParseError::unsupported(
                        "CHANGEFEED and versioned history are deferred",
                        self.peek().span,
                    ));
                } else {
                    return Err(self.unexpected("COMMENT after DROP"));
                }
            } else if self.at(&TokenKind::Compact) {
                return Err(ParseError::unsupported(
                    "table-keyspace COMPACT is not exposed by the pinned engine",
                    self.peek().span,
                ));
            } else if self.at(&TokenKind::Changefeed) {
                return Err(ParseError::unsupported(
                    "CHANGEFEED and versioned history are deferred",
                    self.peek().span,
                ));
            }
            if self.at(&TokenKind::Permissions) {
                permissions = self.parse_schema_permissions()?;
            }
            if self.at(&TokenKind::Comment) {
                let value = self.parse_optional_comment()?.expect("COMMENT was present");
                comment = TableCommentChange::Set(value.value);
            }
            if mode.is_none()
                && permissions.is_none()
                && matches!(comment, TableCommentChange::Unchanged)
            {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidCombination {
                        what: "ALTER TABLE requires a supported clause",
                    },
                    name.span,
                ));
            }
            return Ok(Statement::AlterTable(AlterTableStatement {
                span: Span::new(start.offset, self.previous_end() - start.offset),
                if_exists,
                name,
                mode,
                permissions,
                comment,
            }));
        }
        if self.eat(&TokenKind::Function) {
            let name = self.parse_custom_function_name()?;
            let permissions = self.parse_schema_permissions()?.ok_or_else(|| {
                ParseError::new(
                    ParseErrorKind::InvalidCombination {
                        what: "ALTER FUNCTION requires PERMISSIONS",
                    },
                    name.last().expect("name is nonempty").span,
                )
            })?;
            self.validate_simple_permissions(&permissions, "function")?;
            return Ok(Statement::AlterFunction(AlterFunctionStatement {
                span: Span::new(start.offset, self.previous_end() - start.offset),
                name,
                permissions,
            }));
        }
        if !self.eat(&TokenKind::Param) {
            return Err(ParseError::unsupported(
                "Phase 15 ALTER currently supports PARAM and FUNCTION",
                self.peek().span,
            ));
        }
        let name = self.expect_parameter("a database parameter")?;
        let value = if self.eat(&TokenKind::Value) {
            Some(self.parse_expression()?)
        } else {
            None
        };
        let permissions = if self.at(&TokenKind::Permissions) {
            self.parse_schema_permissions()?
        } else {
            None
        };
        if let Some(permissions) = &permissions {
            self.validate_simple_permissions(permissions, "parameter")?;
        }
        if value.is_none() && permissions.is_none() {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "ALTER PARAM requires VALUE, PERMISSIONS, or both",
                },
                name.span,
            ));
        }
        Ok(Statement::AlterParam(AlterParamStatement {
            span: Span::new(start.offset, self.previous_end() - start.offset),
            name,
            value,
            permissions,
        }))
    }

    fn parse_alter_event(&mut self, start: Span) -> Result<AlterEventStatement, ParseError> {
        self.expect_ident_keyword("event", "keyword EVENT")?;
        let if_exists = if let Some(if_token) = self.take(&TokenKind::If) {
            let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF")?;
            Some(if_token.span.union(exists.span))
        } else {
            None
        };
        let name = self.expect_identifier("an event name")?;
        self.expect(&TokenKind::On, "keyword ON")?;
        let table_keyword = self.take(&TokenKind::Table).map(|token| token.span);
        let table = self.expect_identifier("an event table")?;
        let mut changes = AlterEventChanges::default();
        loop {
            if self.eat_ident_keyword("async") {
                return Err(ParseError::unsupported(
                    "asynchronous events are outside the synchronous Phase 15 event contract",
                    self.previous_span(),
                ));
            }
            if self.eat(&TokenKind::Drop) {
                if self.eat_ident_keyword("async") {
                    return Err(ParseError::unsupported(
                        "asynchronous events are outside the synchronous Phase 15 event contract",
                        self.previous_span(),
                    ));
                }
                if self.eat_ident_keyword("when") {
                    if changes.condition.is_some() {
                        return Err(self.duplicate_clause("WHEN"));
                    }
                    changes.condition = Some(None);
                    continue;
                }
                if self.eat_ident_keyword("then") {
                    if changes.action.is_some() {
                        return Err(self.duplicate_clause("THEN"));
                    }
                    changes.action = Some(None);
                    continue;
                }
                if self.eat(&TokenKind::Comment) {
                    if changes.comment.is_some() {
                        return Err(self.duplicate_clause("COMMENT"));
                    }
                    changes.comment = Some(None);
                    continue;
                }
                return Err(self.unexpected("WHEN, THEN, COMMENT, or ASYNC after DROP"));
            }
            if self.eat_ident_keyword("when") {
                if changes.condition.is_some() {
                    return Err(self.duplicate_clause("WHEN"));
                }
                changes.condition = Some(Some(self.parse_expression()?));
                continue;
            }
            if self.eat_ident_keyword("then") {
                if changes.action.is_some() {
                    return Err(self.duplicate_clause("THEN"));
                }
                changes.action = Some(Some(self.parse_event_action()?));
                continue;
            }
            if self.at(&TokenKind::Comment) {
                if changes.comment.is_some() {
                    return Err(self.duplicate_clause("COMMENT"));
                }
                changes.comment = Some(Some(
                    self.parse_optional_comment()?
                        .expect("COMMENT was present")
                        .value,
                ));
                continue;
            }
            break;
        }
        if changes.condition.is_none() && changes.action.is_none() && changes.comment.is_none() {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "ALTER EVENT requires at least one supported clause",
                },
                table.span,
            ));
        }
        Ok(AlterEventStatement {
            span: Span::new(start.offset, self.previous_end() - start.offset),
            if_exists,
            name,
            table_keyword,
            table,
            changes,
        })
    }

    fn parse_remove(&mut self) -> Result<Statement, ParseError> {
        if matches!(&self.tokens[self.position + 1].kind, TokenKind::Ident(value) if value.eq_ignore_ascii_case("sequence"))
        {
            return Err(ParseError::unsupported(
                "SurrealDB sequence allocation cannot preserve its non-rollback semantics on serialized stable WAL",
                self.tokens[self.position + 1].span,
            ));
        }
        if matches!(&self.tokens[self.position + 1].kind, TokenKind::Ident(value) if value.eq_ignore_ascii_case("api"))
        {
            return Err(ParseError::unsupported(
                "REMOVE API execution belongs to the authenticated Phase 19 server",
                self.tokens[self.position + 1].span,
            ));
        }
        if matches!(&self.tokens[self.position + 1].kind, TokenKind::Ident(value) if value.eq_ignore_ascii_case("event"))
        {
            return self.parse_remove_event().map(Statement::RemoveEvent);
        }
        if self.at_offset(1, &TokenKind::Field) {
            return self.parse_remove_field().map(Statement::RemoveField);
        }
        if self.at_offset(1, &TokenKind::Table) {
            return self.parse_remove_table().map(Statement::RemoveTable);
        }
        if self.at_offset(1, &TokenKind::Function) {
            return self.parse_remove_function().map(Statement::RemoveFunction);
        }
        if self.at_offset(1, &TokenKind::Param) {
            return self.parse_remove_param().map(Statement::RemoveParam);
        }
        self.parse_index_maintenance(false)
            .map(Statement::RemoveIndex)
    }

    fn parse_remove_event(&mut self) -> Result<RemoveEventStatement, ParseError> {
        let start = self.expect(&TokenKind::Remove, "keyword REMOVE")?.span;
        self.expect_ident_keyword("event", "keyword EVENT")?;
        let if_exists = if let Some(if_token) = self.take(&TokenKind::If) {
            let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF")?;
            Some(if_token.span.union(exists.span))
        } else {
            None
        };
        let name = self.expect_identifier("an event name")?;
        self.expect(&TokenKind::On, "keyword ON")?;
        let table_keyword = self.take(&TokenKind::Table).map(|token| token.span);
        let table = self.expect_identifier("an event table")?;
        Ok(RemoveEventStatement {
            span: start.union(table.span),
            if_exists,
            name,
            table_keyword,
            table,
        })
    }

    fn parse_remove_field(&mut self) -> Result<RemoveFieldStatement, ParseError> {
        let start = self.expect(&TokenKind::Remove, "keyword REMOVE")?.span;
        self.expect(&TokenKind::Field, "keyword FIELD")?;
        let if_exists = if let Some(if_token) = self.take(&TokenKind::If) {
            let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF")?;
            Some(if_token.span.union(exists.span))
        } else {
            None
        };
        let path = self.parse_field_path()?;
        self.expect(&TokenKind::On, "keyword ON")?;
        let table_keyword = self.take(&TokenKind::Table).map(|token| token.span);
        let table = self.expect_identifier("a table name")?;
        Ok(RemoveFieldStatement {
            span: start.union(table.span),
            if_exists,
            path,
            table_keyword,
            table,
        })
    }

    fn parse_remove_table(&mut self) -> Result<RemoveTableStatement, ParseError> {
        let start = self.expect(&TokenKind::Remove, "keyword REMOVE")?.span;
        self.expect(&TokenKind::Table, "keyword TABLE")?;
        let if_exists = if let Some(if_token) = self.take(&TokenKind::If) {
            let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF")?;
            Some(if_token.span.union(exists.span))
        } else {
            None
        };
        let name = self.expect_identifier("a table name")?;
        Ok(RemoveTableStatement {
            span: start.union(name.span),
            if_exists,
            name,
        })
    }

    fn parse_remove_function(&mut self) -> Result<RemoveFunctionStatement, ParseError> {
        let start = self.expect(&TokenKind::Remove, "keyword REMOVE")?.span;
        self.expect(&TokenKind::Function, "keyword FUNCTION")?;
        let if_exists = if let Some(if_token) = self.take(&TokenKind::If) {
            let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF")?;
            Some(if_token.span.union(exists.span))
        } else {
            None
        };
        let name = self.parse_custom_function_name()?;
        let end = name.last().expect("name is nonempty").span;
        Ok(RemoveFunctionStatement {
            span: start.union(end),
            if_exists,
            name,
        })
    }

    fn parse_remove_param(&mut self) -> Result<RemoveParamStatement, ParseError> {
        let start = self.expect(&TokenKind::Remove, "keyword REMOVE")?.span;
        self.expect(&TokenKind::Param, "keyword PARAM")?;
        let if_exists = if let Some(if_token) = self.take(&TokenKind::If) {
            let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF")?;
            Some(if_token.span.union(exists.span))
        } else {
            None
        };
        let name = self.expect_parameter("a database parameter")?;
        Ok(RemoveParamStatement {
            span: start.union(name.span),
            if_exists,
            name,
        })
    }

    fn parse_info(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(&TokenKind::Info, "keyword INFO")?.span;
        self.expect(&TokenKind::For, "keyword FOR after INFO")?;
        if self.eat(&TokenKind::Table) {
            let table = self.expect_identifier("a table name")?;
            return Ok(Statement::InfoTable(InfoTableStatement {
                span: start.union(table.span),
                table,
            }));
        }
        let target = self.expect_identifier("DB or DATABASE")?;
        if target.value.eq_ignore_ascii_case("sequence") {
            return Err(ParseError::unsupported(
                "SurrealDB sequence allocation cannot preserve its non-rollback semantics on serialized stable WAL",
                target.span,
            ));
        }
        if target.value.eq_ignore_ascii_case("api") {
            return Err(ParseError::unsupported(
                "INFO API execution belongs to the authenticated Phase 19 server",
                target.span,
            ));
        }
        if matches!(
            target.value.to_ascii_lowercase().as_str(),
            "db" | "database"
        ) && self.at(&TokenKind::Dot)
        {
            let dot = self.advance().span;
            let collection = self.expect_identifier("an INFO database collection")?;
            if collection.value.eq_ignore_ascii_case("sequences") {
                return Err(ParseError::unsupported(
                    "SurrealDB sequence allocation cannot preserve its non-rollback semantics on serialized stable WAL",
                    dot.union(collection.span),
                ));
            }
            if collection.value.eq_ignore_ascii_case("apis") {
                return Err(ParseError::unsupported(
                    "INFO API execution belongs to the authenticated Phase 19 server",
                    dot.union(collection.span),
                ));
            }
            return Err(ParseError::unsupported(
                "this INFO database collection is outside the active FastDB grammar",
                dot.union(collection.span),
            ));
        }
        if !matches!(
            target.value.to_ascii_lowercase().as_str(),
            "db" | "database"
        ) {
            return Err(ParseError::unsupported(
                "Phase 15 INFO currently supports the database target",
                target.span,
            ));
        }
        Ok(Statement::InfoDatabase(InfoDatabaseStatement {
            span: start.union(target.span),
        }))
    }

    fn parse_explain(&mut self) -> Result<ExplainStatement, ParseError> {
        let start = self.expect(&TokenKind::Explain, "keyword EXPLAIN")?.span;
        let analyze = self.take(&TokenKind::Analyze).map(|token| token.span);
        let full = self.take(&TokenKind::Full).map(|token| token.span);
        let format_json = if self.eat(&TokenKind::Format) {
            Some(
                self.expect(&TokenKind::Json, "keyword JSON after FORMAT")?
                    .span,
            )
        } else {
            None
        };
        if !self.at(&TokenKind::Select) {
            return Err(ParseError::unsupported(
                "Phase 6 EXPLAIN accepts SELECT only",
                self.peek().span,
            ));
        }
        let select = self.parse_select()?;
        Ok(ExplainStatement {
            span: start.union(select.span),
            analyze,
            full,
            format_json,
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
        let if_not_exists = if let Some(if_token) = self.take(&TokenKind::If) {
            self.expect(&TokenKind::Not, "keyword NOT after IF")?;
            let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF NOT")?;
            Some(if_token.span.union(exists.span))
        } else {
            None
        };
        let overwrite = self.take(&TokenKind::Overwrite).map(|token| token.span);
        if let (Some(_), Some(overwrite_span)) = (if_not_exists, overwrite) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "DEFINE TABLE cannot combine IF NOT EXISTS and OVERWRITE",
                },
                overwrite_span,
            ));
        }
        let name = self.expect_identifier("a table name")?;
        let drop = self.take(&TokenKind::Drop).map(|token| token.span);
        let mut mode = if let Some(token) = self.take(&TokenKind::Schemaless) {
            Some(Spanned::new(TableMode::Schemaless, token.span))
        } else if let Some(token) = self.take(&TokenKind::Schemafull) {
            Some(Spanned::new(TableMode::Schemafull, token.span))
        } else {
            None
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
            } else if matches!(&self.peek().kind, TokenKind::Ident(value) if value.eq_ignore_ascii_case("any"))
            {
                let any = self.advance().clone();
                return Err(ParseError::unsupported(
                    "TYPE ANY requires a mixed normal/relation physical provider",
                    type_token.span.union(any.span),
                ));
            } else {
                return Err(self.unexpected("ANY, NORMAL, or RELATION after TYPE"));
            }
        } else {
            TableKindSyntax::Normal { type_span: None }
        };
        if mode.is_none() {
            if let Some(token) = self.take(&TokenKind::Schemaless) {
                mode = Some(Spanned::new(TableMode::Schemaless, token.span));
            } else if let Some(token) = self.take(&TokenKind::Schemafull) {
                mode = Some(Spanned::new(TableMode::Schemafull, token.span));
            }
        }
        let mode = mode
            .unwrap_or_else(|| Spanned::new(TableMode::Schemaless, Span::new(name.span.end(), 0)));
        if self.at(&TokenKind::Changefeed) {
            return Err(ParseError::unsupported(
                "CHANGEFEED and versioned history are deferred",
                self.peek().span,
            ));
        }
        if self.at(&TokenKind::View) {
            return Err(ParseError::unsupported(
                "DEFINE TABLE VIEW spelling is not part of the characterized v3.1.5 surface",
                self.peek().span,
            ));
        }
        let view = if let Some(as_token) = self.take(&TokenKind::As) {
            if drop.is_some()
                || !matches!(kind, TableKindSyntax::Normal { .. })
                || mode.value != TableMode::Schemaless
            {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidCombination {
                        what: "table views must be non-DROP, NORMAL, and SCHEMALESS",
                    },
                    as_token.span,
                ));
            }
            Some(Box::new(self.parse_select()?))
        } else {
            None
        };
        let permissions = self
            .parse_schema_permissions()?
            .unwrap_or(SchemaPermissions::None);
        let comment = self.parse_optional_comment()?;
        let end = comment
            .as_ref()
            .map_or_else(
                || match &kind {
                    TableKindSyntax::Normal {
                        type_span: Some(span),
                    } => span.end(),
                    TableKindSyntax::Normal { type_span: None } => {
                        mode.span.end().max(name.span.end())
                    }
                    TableKindSyntax::Relation(relation) => relation.span.end(),
                },
                |comment| comment.span.end(),
            )
            .max(self.previous_end());
        Ok(DefineTableStatement {
            span: Span::new(start.offset, end - start.offset),
            if_not_exists,
            overwrite,
            name,
            drop,
            mode,
            kind,
            view,
            permissions,
            comment,
        })
    }

    fn parse_optional_comment(&mut self) -> Result<Option<Spanned<String>>, ParseError> {
        if !self.eat(&TokenKind::Comment) {
            return Ok(None);
        }
        let token = self.peek().clone();
        let TokenKind::String(value) = token.kind else {
            return Err(self.unexpected_at(&token, "a string after COMMENT"));
        };
        self.position += 1;
        Ok(Some(Spanned::new(value, token.span)))
    }

    fn parse_define_field(&mut self, start: Span) -> Result<DefineFieldStatement, ParseError> {
        self.expect(&TokenKind::Field, "keyword FIELD")?;
        let if_not_exists = if let Some(if_token) = self.take(&TokenKind::If) {
            self.expect(&TokenKind::Not, "keyword NOT after IF")?;
            let exists = self.expect(&TokenKind::Exists, "keyword EXISTS after IF NOT")?;
            Some(if_token.span.union(exists.span))
        } else {
            None
        };
        let overwrite = self.take(&TokenKind::Overwrite).map(|token| token.span);
        if let (Some(_), Some(overwrite_span)) = (if_not_exists, overwrite) {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "DEFINE FIELD cannot combine IF NOT EXISTS and OVERWRITE",
                },
                overwrite_span,
            ));
        }
        let path = self.parse_field_path()?;
        self.expect(&TokenKind::On, "keyword ON")?;
        let table_keyword = self.take(&TokenKind::Table).map(|token| token.span);
        let table = self.expect_identifier("a table name")?;
        let ty = if self.eat(&TokenKind::Type) {
            self.parse_schema_type()?
        } else {
            SchemaType {
                span: Span::new(table.span.end(), 0),
                kind: SchemaTypeKind::Any,
            }
        };
        let flexible = if self.at_ident_keyword("flexible") {
            Some(self.advance().span)
        } else {
            None
        };
        let mut default = None;
        let mut value = None;
        let mut assert = None;
        let mut readonly = None;
        let mut reference = None;
        let mut reference_action = None;
        let mut permissions = None;
        let mut comment = None;
        loop {
            if let Some(token) = self.take(&TokenKind::Default) {
                if default.is_some() {
                    return Err(ParseError::new(
                        ParseErrorKind::DuplicateClause { clause: "DEFAULT" },
                        token.span,
                    ));
                }
                let always = self.take(&TokenKind::Always).map(|token| token.span);
                let expression = self.parse_expression()?;
                default = Some(FieldDefaultClause {
                    span: token.span.union(expression.span),
                    always,
                    value: expression,
                });
            } else if let Some(token) = self.take(&TokenKind::Value) {
                if value.is_some() {
                    return Err(ParseError::new(
                        ParseErrorKind::DuplicateClause { clause: "VALUE" },
                        token.span,
                    ));
                }
                value = Some(self.parse_expression()?);
            } else if let Some(token) = self.take(&TokenKind::Assert) {
                if assert.is_some() {
                    return Err(ParseError::new(
                        ParseErrorKind::DuplicateClause { clause: "ASSERT" },
                        token.span,
                    ));
                }
                assert = Some(self.parse_expression()?);
            } else if let Some(token) = self.take(&TokenKind::Readonly) {
                if readonly.replace(token.span).is_some() {
                    return Err(ParseError::new(
                        ParseErrorKind::DuplicateClause { clause: "READONLY" },
                        token.span,
                    ));
                }
            } else if let Some(token) = self.take(&TokenKind::Reference) {
                if reference.replace(token.span).is_some() {
                    return Err(ParseError::new(
                        ParseErrorKind::DuplicateClause {
                            clause: "REFERENCE",
                        },
                        token.span,
                    ));
                }
                reference_action = self.parse_reference_delete_action()?;
            } else if self.at(&TokenKind::Permissions) {
                if permissions.is_some() {
                    return Err(self.duplicate_clause("PERMISSIONS"));
                }
                permissions = self.parse_schema_permissions()?;
            } else if self.at(&TokenKind::Comment) {
                if comment.is_some() {
                    return Err(self.duplicate_clause("COMMENT"));
                }
                comment = self.parse_optional_comment()?;
            } else {
                break;
            }
        }
        let permissions = permissions.unwrap_or(SchemaPermissions::Full);
        self.validate_field_permissions(&permissions)?;
        Ok(DefineFieldStatement {
            span: Span::new(start.offset, self.previous_end() - start.offset),
            if_not_exists,
            overwrite,
            path,
            table_keyword,
            table,
            ty,
            flexible,
            default,
            value,
            assert,
            readonly,
            reference,
            reference_action,
            permissions,
            comment,
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
        if matches!(
            self.peek().kind,
            TokenKind::Range | TokenKind::RangeInclusive
        ) {
            let operator = self.advance().clone();
            let inclusive = matches!(operator.kind, TokenKind::RangeInclusive);
            let end = if record_id_part_starts(&self.peek().kind) {
                Some(self.parse_record_id_part()?)
            } else {
                None
            };
            let span = end.as_ref().map_or_else(
                || table.span.union(operator.span),
                |end| table.span.union(end.span),
            );
            return Ok(Target::RecordRange(RecordRangeTarget {
                span,
                table,
                start: None,
                end,
                inclusive,
                operator_span: operator.span,
            }));
        }
        let id = self.parse_record_id_part()?;
        if matches!(
            self.peek().kind,
            TokenKind::Range | TokenKind::RangeInclusive
        ) {
            let operator = self.advance().clone();
            let inclusive = matches!(operator.kind, TokenKind::RangeInclusive);
            let end = if record_id_part_starts(&self.peek().kind) {
                Some(self.parse_record_id_part()?)
            } else {
                None
            };
            let span = end.as_ref().map_or_else(
                || table.span.union(operator.span),
                |end| table.span.union(end.span),
            );
            return Ok(Target::RecordRange(RecordRangeTarget {
                span,
                table,
                start: Some(id),
                end,
                inclusive,
                operator_span: operator.span,
            }));
        }
        let span = table.span.union(id.span);
        Ok(Target::Record(RecordId { span, table, id }))
    }

    fn parse_mutation_target(&mut self) -> Result<Target, ParseError> {
        if matches!(
            self.peek().kind,
            TokenKind::LeftBracket | TokenKind::Parameter(_) | TokenKind::LeftParen
        ) {
            return self.parse_expression().map(Target::Expression);
        }
        self.parse_target()
    }

    fn parse_record_id_part(&mut self) -> Result<RecordIdPart, ParseError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::LeftBracket | TokenKind::LeftBrace => {
                let expression = self.parse_prefix_expression()?;
                if !is_complex_record_id_expression(&expression) {
                    return Err(ParseError::new(
                        ParseErrorKind::InvalidCombination {
                            what:
                                "complex record IDs must contain only literal array/object values",
                        },
                        expression.span,
                    ));
                }
                Ok(RecordIdPart {
                    span: expression.span,
                    kind: RecordIdPartKind::Complex(Box::new(expression)),
                })
            }
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
            kind => {
                if let Some(value) = keyword_object_key(&kind) {
                    self.position += 1;
                    Ok(RecordIdPart {
                        span: token.span,
                        kind: RecordIdPartKind::Bare(value),
                    })
                } else {
                    Err(self.unexpected(
                        "a bare, backtick-quoted, integer, typed-UUID, array, or object record-ID component",
                    ))
                }
            }
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
        let first = self.advance().clone();
        let (operator, operator_span) = match first.kind {
            TokenKind::Equal => (AssignmentOperator::Set, first.span),
            TokenKind::Plus | TokenKind::Minus if self.at(&TokenKind::Equal) => {
                let equal = self.advance().span;
                let operator = if matches!(first.kind, TokenKind::Plus) {
                    AssignmentOperator::Add
                } else {
                    AssignmentOperator::Subtract
                };
                (operator, first.span.union(equal))
            }
            _ => return Err(self.unexpected_at(&first, "'=', '+=', or '-='")),
        };
        let value = self.parse_expression()?;
        Ok(Assignment {
            span: path.span.union(value.span),
            path,
            operator: Spanned::new(operator, operator_span),
            value,
        })
    }

    fn parse_projections(&mut self) -> Result<(ProjectionList, bool), ParseError> {
        if let Some(star) = self.take(&TokenKind::Star) {
            if self.eat(&TokenKind::Comma) {
                let mut fields = vec![self.parse_projection()?];
                while self.eat(&TokenKind::Comma) {
                    let projection = self.parse_projection()?;
                    self.check_element_count(fields.len() + 1, projection.span)?;
                    fields.push(projection);
                }
                return Ok((ProjectionList::Fields(fields), true));
            }
            return Ok((ProjectionList::All(star.span), false));
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
        Ok((ProjectionList::Fields(fields), false))
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
            let mut collate = None;
            let mut numeric = None;
            let mut direction = None;
            loop {
                if let Some(token) = self.take(&TokenKind::Collate) {
                    if collate.replace(token.span).is_some() {
                        return Err(self.duplicate_clause("COLLATE"));
                    }
                } else if let Some(token) = self.take(&TokenKind::Numeric) {
                    if numeric.replace(token.span).is_some() {
                        return Err(self.duplicate_clause("NUMERIC"));
                    }
                } else if let Some(token) = self.take(&TokenKind::Asc) {
                    if direction
                        .replace(Spanned::new(OrderDirection::Ascending, token.span))
                        .is_some()
                    {
                        return Err(self.duplicate_clause("ASC/DESC"));
                    }
                } else if let Some(token) = self.take(&TokenKind::Desc) {
                    if direction
                        .replace(Spanned::new(OrderDirection::Descending, token.span))
                        .is_some()
                    {
                        return Err(self.duplicate_clause("ASC/DESC"));
                    }
                } else {
                    break;
                }
            }
            let direction = direction.unwrap_or_else(|| {
                Spanned::new(OrderDirection::Ascending, Span::new(path.span.end(), 0))
            });
            let end = [collate, numeric, Some(direction.span)]
                .into_iter()
                .flatten()
                .max_by_key(|span| span.end())
                .unwrap_or(path.span);
            let span = path.span.union(end);
            self.check_element_count(terms.len() + 1, span)?;
            terms.push(OrderBy {
                span,
                path,
                direction,
                collate,
                numeric,
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
            TokenKind::After => ReturnKind::After,
            TokenKind::None => ReturnKind::None,
            TokenKind::Before => ReturnKind::Before,
            TokenKind::Diff => ReturnKind::Diff,
            TokenKind::Value => ReturnKind::Value(self.parse_expression()?),
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
            span: return_span.union(match &kind {
                ReturnKind::Value(expression) => expression.span,
                _ => token.span,
            }),
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

    fn validate_collection_dimension(value: &NonnegativeInteger) -> Result<(), ParseError> {
        if value.value == 0 || value.value > 65_536 {
            return Err(ParseError::new(
                ParseErrorKind::InvalidCombination {
                    what: "typed collection length must be between 1 and 65,536",
                },
                value.span,
            ));
        }
        Ok(())
    }

    fn parse_schema_type(&mut self) -> Result<SchemaType, ParseError> {
        let first = self.parse_schema_type_atom()?;
        if !self.at(&TokenKind::Pipe) {
            return Ok(first);
        }
        let mut span = first.span;
        let mut variants = vec![first];
        while self.eat(&TokenKind::Pipe) {
            let variant = self.parse_schema_type_atom()?;
            span = span.union(variant.span);
            variants.push(variant);
            self.check_element_count(variants.len(), span)?;
        }
        Ok(SchemaType {
            span,
            kind: SchemaTypeKind::Union(variants),
        })
    }

    fn parse_schema_type_atom(&mut self) -> Result<SchemaType, ParseError> {
        let token = self.advance().clone();
        let kind = match token.kind {
            TokenKind::None => SchemaTypeKind::Literal(SchemaTypeLiteral::None),
            TokenKind::Null => SchemaTypeKind::Literal(SchemaTypeLiteral::Null),
            TokenKind::True => SchemaTypeKind::Literal(SchemaTypeLiteral::Bool(true)),
            TokenKind::False => SchemaTypeKind::Literal(SchemaTypeLiteral::Bool(false)),
            TokenKind::String(value) => SchemaTypeKind::Literal(SchemaTypeLiteral::String(value)),
            TokenKind::Number(value) => {
                let expression = parse_number_expression(value, token.span)?;
                match expression.kind {
                    ExprKind::Integer(value) => {
                        SchemaTypeKind::Literal(SchemaTypeLiteral::Integer(value))
                    }
                    ExprKind::Float(value) => {
                        SchemaTypeKind::Literal(SchemaTypeLiteral::Float(value))
                    }
                    _ => unreachable!("number parser returns a numeric expression"),
                }
            }
            TokenKind::Plus | TokenKind::Minus => {
                let sign: i8 = if matches!(token.kind, TokenKind::Minus) {
                    -1
                } else {
                    1
                };
                let number = self.advance().clone();
                let TokenKind::Number(value) = number.kind else {
                    return Err(self.unexpected_at(&number, "a numeric literal schema type"));
                };
                let span = token.span.union(number.span);
                if !value.contains(['.', 'e', 'E']) {
                    return Ok(SchemaType {
                        span,
                        kind: SchemaTypeKind::Literal(SchemaTypeLiteral::Integer(
                            parse_signed_integer(&value, sign, span)?,
                        )),
                    });
                }
                let expression = parse_number_expression(value, span)?;
                return Ok(SchemaType {
                    span,
                    kind: SchemaTypeKind::Literal(match expression.kind {
                        ExprKind::Integer(_) => {
                            unreachable!("integer schema literals return before float parsing")
                        }
                        ExprKind::Float(value) => SchemaTypeLiteral::Float(value * sign as f64),
                        _ => unreachable!("number parser returns a numeric expression"),
                    }),
                });
            }
            TokenKind::Ident(ref value) if value.eq_ignore_ascii_case("any") => SchemaTypeKind::Any,
            TokenKind::BoolType => SchemaTypeKind::Bool,
            TokenKind::IntType => SchemaTypeKind::Int,
            TokenKind::FloatType => SchemaTypeKind::Float,
            TokenKind::NumberType => SchemaTypeKind::Number,
            TokenKind::DecimalType => SchemaTypeKind::Decimal,
            TokenKind::StringType => SchemaTypeKind::String,
            TokenKind::BytesType => SchemaTypeKind::Bytes,
            TokenKind::DatetimeType => SchemaTypeKind::Datetime,
            TokenKind::DurationType => SchemaTypeKind::Duration,
            TokenKind::UuidType => SchemaTypeKind::Uuid,
            TokenKind::RegexType => SchemaTypeKind::Regex,
            TokenKind::FileType => SchemaTypeKind::File,
            TokenKind::Table => SchemaTypeKind::Table,
            TokenKind::ObjectType => SchemaTypeKind::Object,
            TokenKind::ArrayType if self.at(&TokenKind::Less) => {
                self.advance();
                self.enter_depth(token.span)?;
                let element_result = self.parse_schema_type();
                self.leave_depth();
                let element = element_result?;
                let dimension = if self.eat(&TokenKind::Comma) {
                    let dimension = self.parse_nonnegative_integer()?;
                    Self::validate_collection_dimension(&dimension)?;
                    Some(dimension)
                } else {
                    None
                };
                let close = self.expect(&TokenKind::Greater, "'>' after vector dimension")?;
                let kind = match (&element.kind, dimension) {
                    (SchemaTypeKind::Float, Some(dimension)) => {
                        SchemaTypeKind::FixedFloatArray(dimension)
                    }
                    (_, length) => SchemaTypeKind::TypedArray {
                        element: Box::new(element),
                        length,
                    },
                };
                return Ok(SchemaType {
                    span: token.span.union(close.span),
                    kind,
                });
            }
            TokenKind::ArrayType => SchemaTypeKind::Array,
            TokenKind::Set if self.at(&TokenKind::Less) => {
                self.advance();
                self.enter_depth(token.span)?;
                let element_result = self.parse_schema_type();
                self.leave_depth();
                let element = element_result?;
                let length = if self.eat(&TokenKind::Comma) {
                    let length = self.parse_nonnegative_integer()?;
                    Self::validate_collection_dimension(&length)?;
                    Some(length)
                } else {
                    None
                };
                let close = self.expect(&TokenKind::Greater, "'>' after set type")?;
                return Ok(SchemaType {
                    span: token.span.union(close.span),
                    kind: SchemaTypeKind::Set {
                        element: Some(Box::new(element)),
                        length,
                    },
                });
            }
            TokenKind::Set => SchemaTypeKind::Set {
                element: None,
                length: None,
            },
            TokenKind::RangeType => SchemaTypeKind::Range,
            TokenKind::RecordType if self.eat(&TokenKind::Less) => {
                let mut tables = Vec::new();
                loop {
                    let table = self.expect_identifier("a table name in record<...>")?;
                    tables.push(table);
                    self.check_element_count(tables.len(), token.span)?;
                    if !self.eat(&TokenKind::Pipe) {
                        break;
                    }
                }
                let close = self.expect(&TokenKind::Greater, "'>' after record tables")?;
                return Ok(SchemaType {
                    span: token.span.union(close.span),
                    kind: SchemaTypeKind::Record { tables },
                });
            }
            TokenKind::RecordType => SchemaTypeKind::Record { tables: Vec::new() },
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
            if self.at(&TokenKind::LeftBracket) {
                left = self.parse_access_expression(left)?;
                continue;
            }
            if self.at(&TokenKind::Dot) && self.at_offset(1, &TokenKind::LeftBrace) {
                self.advance();
                left = self.parse_destructure_expression(left)?;
                continue;
            }
            if self.at(&TokenKind::Dot) && !self.at_offset(1, &TokenKind::Star) {
                self.advance();
                let field = self.expect_path_segment("a path segment after '.'")?;
                let span = left.span.union(field.span);
                left = Expr::new(
                    ExprKind::Access {
                        target: Box::new(left),
                        accessor: Accessor::Field(field),
                    },
                    span,
                );
                continue;
            }
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
            if let TokenKind::UnsupportedOperator(operator) = self.peek().kind {
                return Err(ParseError::unsupported(
                    excluded_operator_description(operator),
                    self.peek().span,
                ));
            }
            if matches!(
                self.peek().kind,
                TokenKind::Range | TokenKind::RangeInclusive
            ) {
                const LEFT_BP: u8 = 15;
                if LEFT_BP < minimum_binding_power {
                    break;
                }
                let token = self.advance().clone();
                let inclusive = matches!(token.kind, TokenKind::RangeInclusive);
                let end = if self.range_has_end() {
                    Some(Box::new(self.parse_expression_bp(LEFT_BP + 1)?))
                } else {
                    None
                };
                let span = end.as_deref().map_or_else(
                    || left.span.union(token.span),
                    |end| left.span.union(end.span),
                );
                left = Expr::new(
                    ExprKind::Range(RangeExpr {
                        start: Some(Box::new(left)),
                        end,
                        inclusive,
                        operator_span: token.span,
                    }),
                    span,
                );
                continue;
            }
            let Some((operator, left_bp, right_bp, token_count)) = self.binary_operator() else {
                break;
            };
            if left_bp < minimum_binding_power {
                break;
            }
            let first = self.advance().clone();
            let last = if token_count == 2 {
                self.advance().clone()
            } else {
                first.clone()
            };
            let right = self.parse_expression_bp(right_bp)?;
            let span = left.span.union(right.span);
            left = Expr::new(
                ExprKind::Binary {
                    left: Box::new(left),
                    operator: Spanned::new(operator, first.span.union(last.span)),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_prefix_expression(&mut self) -> Result<Expr, ParseError> {
        let token = self.peek().clone();
        if matches!(token.kind, TokenKind::Pipe) {
            return self.parse_closure_expression();
        }
        if (self.at_offset(1, &TokenKind::DoubleColon) || self.at_offset(1, &TokenKind::LeftParen))
            && function_segment_value(&token.kind).is_some()
        {
            return self.parse_identifier_expression();
        }
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
                let operand_result = self.parse_expression_bp(19);
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
            TokenKind::None => {
                self.position += 1;
                Ok(Expr::new(ExprKind::None, token.span))
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
            TokenKind::Duration(value) => {
                self.position += 1;
                Ok(Expr::new(ExprKind::Duration(value), token.span))
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
            kind if keyword_object_key(&kind).is_some() => self.parse_identifier_expression(),
            TokenKind::LeftParen => self.parse_parenthesized_expression(),
            TokenKind::LeftBracket => self.parse_array_expression(),
            TokenKind::LeftBrace if self.brace_starts_object() => self.parse_object_expression(),
            TokenKind::LeftBrace => self.parse_destructure_list_expression(),
            TokenKind::Less => self.parse_cast_expression(),
            TokenKind::Range | TokenKind::RangeInclusive => {
                self.position += 1;
                let inclusive = matches!(token.kind, TokenKind::RangeInclusive);
                let end = if self.range_has_end() {
                    Some(Box::new(self.parse_expression_bp(16)?))
                } else {
                    None
                };
                let span = end
                    .as_deref()
                    .map_or(token.span, |end| token.span.union(end.span));
                Ok(Expr::new(
                    ExprKind::Range(RangeExpr {
                        start: None,
                        end,
                        inclusive,
                        operator_span: token.span,
                    }),
                    span,
                ))
            }
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

    fn parse_cast_expression(&mut self) -> Result<Expr, ParseError> {
        let open = self.expect(&TokenKind::Less, "'<' before cast type")?.span;
        self.enter_depth(open)?;
        let ty_result = self.parse_schema_type();
        self.leave_depth();
        let ty = ty_result?;
        let close = self
            .expect(&TokenKind::Greater, "'>' after cast type")?
            .span;
        self.enter_depth(open)?;
        let value_result = self.parse_expression_bp(14);
        self.leave_depth();
        let value = value_result?;
        let span = open.union(value.span);
        Ok(Expr::new(
            ExprKind::Cast {
                ty: SchemaType {
                    span: open.union(close),
                    kind: ty.kind,
                },
                value: Box::new(value),
            },
            span,
        ))
    }

    fn parse_access_expression(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let open = self.expect(&TokenKind::LeftBracket, "'['")?.span;
        self.enter_depth(open)?;
        let accessor_result = if let Some(last) = self.take(&TokenKind::Dollar) {
            Ok(Accessor::Last(last.span))
        } else {
            let expression = self.parse_expression()?;
            match expression.kind {
                ExprKind::Range(range) => Ok(Accessor::Slice {
                    start: range.start,
                    end: range.end,
                    inclusive: range.inclusive,
                    span: range.operator_span,
                }),
                _ => Ok(Accessor::Index(Box::new(expression))),
            }
        };
        self.leave_depth();
        let accessor = accessor_result?;
        let close = self.expect(&TokenKind::RightBracket, "']'")?.span;
        let span = target.span.union(close);
        Ok(Expr::new(
            ExprKind::Access {
                target: Box::new(target),
                accessor,
            },
            span,
        ))
    }

    fn range_has_end(&self) -> bool {
        !matches!(
            self.peek().kind,
            TokenKind::RightBracket
                | TokenKind::RightParen
                | TokenKind::RightBrace
                | TokenKind::Comma
                | TokenKind::Semicolon
                | TokenKind::Eof
                | TokenKind::As
                | TokenKind::Where
                | TokenKind::Order
                | TokenKind::Limit
                | TokenKind::Start
                | TokenKind::Return
        )
    }

    fn binary_operator(&self) -> Option<(BinaryOperator, u8, u8, usize)> {
        if self.at(&TokenKind::Is) {
            return Some(if self.at_offset(1, &TokenKind::Not) {
                (BinaryOperator::NotEqual, 5, 6, 2)
            } else {
                (BinaryOperator::Equal, 5, 6, 1)
            });
        }
        if self.at(&TokenKind::Not) && self.at_offset(1, &TokenKind::In) {
            return Some((BinaryOperator::NotInside, 7, 8, 2));
        }
        binary_binding_power(&self.peek().kind)
            .map(|(operator, left, right)| (operator, left, right, 1))
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
        if self.at(&TokenKind::DoubleColon) {
            return self.parse_namespaced_expression(first);
        }
        if self.at(&TokenKind::LeftParen) {
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

    fn parse_closure_expression(&mut self) -> Result<Expr, ParseError> {
        let open = self.expect(&TokenKind::Pipe, "'|' to start closure")?.span;
        let mut parameters = Vec::new();
        if !self.at(&TokenKind::Pipe) {
            loop {
                let token = self.peek().clone();
                let TokenKind::Parameter(name) = token.kind else {
                    return Err(self.unexpected("a closure parameter such as $value"));
                };
                self.position += 1;
                if parameters
                    .iter()
                    .any(|parameter: &Identifier| parameter.value == name)
                {
                    return Err(ParseError::new(
                        ParseErrorKind::InvalidCombination {
                            what: "duplicate closure parameter",
                        },
                        token.span,
                    ));
                }
                parameters.push(Identifier::new(name, token.span));
                self.check_element_count(parameters.len(), token.span)?;
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        if parameters.is_empty() {
            return Err(self.unexpected("at least one closure parameter"));
        }
        self.expect(&TokenKind::Pipe, "'|' after closure parameters")?;
        self.enter_depth(open)?;
        let body_result = self.parse_expression_bp(0);
        self.leave_depth();
        let body = body_result?;
        let span = open.union(body.span);
        Ok(Expr::new(
            ExprKind::Closure(ClosureExpr {
                parameters,
                body: Box::new(body),
            }),
            span,
        ))
    }

    fn parse_function_call(&mut self, first: Identifier) -> Result<Expr, ParseError> {
        self.parse_function_call_name(vec![first])
    }

    fn parse_namespaced_expression(&mut self, first: Identifier) -> Result<Expr, ParseError> {
        let mut name = vec![first];
        while self.eat(&TokenKind::DoubleColon) {
            let segment = self.expect_function_segment("a function name after '::'")?;
            self.check_element_count(name.len() + 1, segment.span)?;
            name.push(segment);
        }
        if self.at(&TokenKind::LeftParen) {
            return self.parse_function_call_name(name);
        }
        let span = name[0]
            .span
            .union(name.last().expect("name is nonempty").span);
        Ok(Expr::new(ExprKind::NamespacedValue { name }, span))
    }

    fn parse_function_call_name(&mut self, name: Vec<Identifier>) -> Result<Expr, ParseError> {
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

    fn parse_destructure_expression(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let open = self.expect(&TokenKind::LeftBrace, "'{' after '.'")?.span;
        let mut fields = Vec::new();
        if self.at(&TokenKind::RightBrace) {
            return Err(self.unexpected("at least one destructured field"));
        }
        loop {
            let field = self.expect_path_segment("a destructured field")?;
            self.check_element_count(fields.len() + 1, field.span)?;
            fields.push(field);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        let close = self
            .expect(&TokenKind::RightBrace, "'}' after destructured fields")?
            .span;
        let span = target.span.union(close);
        let _ = open;
        Ok(Expr::new(
            ExprKind::Destructure {
                target: Box::new(target),
                fields,
            },
            span,
        ))
    }

    fn parse_destructure_list_expression(&mut self) -> Result<Expr, ParseError> {
        let open = self.expect(&TokenKind::LeftBrace, "'{'")?.span;
        let mut values = Vec::new();
        loop {
            let value = self.parse_expression()?;
            self.check_element_count(values.len() + 1, value.span)?;
            values.push(value);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        let close = self
            .expect(&TokenKind::RightBrace, "'}' after destructuring")?
            .span;
        Ok(Expr::new(
            ExprKind::DestructureList(values),
            open.union(close),
        ))
    }

    fn brace_starts_object(&self) -> bool {
        if self.at_offset(1, &TokenKind::RightBrace) {
            return true;
        }
        let mut depth = 0_usize;
        for token in self.tokens.iter().skip(self.position + 1) {
            match token.kind {
                TokenKind::LeftBrace | TokenKind::LeftBracket | TokenKind::LeftParen => {
                    depth += 1;
                }
                TokenKind::RightBrace if depth == 0 => return false,
                TokenKind::RightBrace | TokenKind::RightBracket | TokenKind::RightParen => {
                    depth = depth.saturating_sub(1);
                }
                TokenKind::Colon if depth == 0 => return true,
                TokenKind::Eof => return false,
                _ => {}
            }
        }
        false
    }

    fn parse_object_fields(&mut self) -> Result<(Vec<ObjectField>, Span), ParseError> {
        let mut fields = Vec::new();
        if let Some(close) = self.take(&TokenKind::RightBrace) {
            return Ok((fields, close.span));
        }
        loop {
            let key_token = self.advance().clone();
            let key_kind = match key_token.kind.clone() {
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
                kind => match keyword_object_key(&kind) {
                    Some(value) => ObjectKeyKind::Identifier(value),
                    None => {
                        return Err(
                            self.unexpected_at(&key_token, "an identifier, keyword, or string key")
                        )
                    }
                },
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
        while self.at(&TokenKind::Dot) && !self.at_offset(1, &TokenKind::LeftBrace) {
            self.advance();
            segments.push(self.expect_path_segment("a path segment after '.'")?);
        }
        let end = segments.last().expect("one path segment").span;
        Ok(FieldPath {
            segments,
            span: start.union(end),
        })
    }

    fn ensure_statement_boundary(&self) -> Result<(), ParseError> {
        if self.at(&TokenKind::Semicolon)
            || self.at(&TokenKind::RightBrace)
            || self.at(&TokenKind::RightParen)
            || self.at(&TokenKind::Eof)
            || (self.event_action_boundary
                && (self.at(&TokenKind::Comment)
                    || self.at(&TokenKind::Drop)
                    || self.at(&TokenKind::Comma)))
        {
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

    fn with_depth<T>(
        &mut self,
        action: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        let span = self.tokens[self.position.saturating_sub(1)].span;
        self.enter_depth(span)?;
        let result = action(self);
        self.leave_depth();
        result
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
        let value = match token.kind.clone() {
            TokenKind::Ident(value) => Some(value),
            kind => keyword_object_key(&kind),
        };
        if let Some(value) = value {
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

    fn at_ident_keyword(&self, expected: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Ident(value) if value.eq_ignore_ascii_case(expected))
    }

    fn eat_ident_keyword(&mut self, expected: &str) -> bool {
        if self.at_ident_keyword(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect_ident_keyword(
        &mut self,
        value: &str,
        expected: &'static str,
    ) -> Result<Token, ParseError> {
        if self.at_ident_keyword(value) {
            let token = self.peek().clone();
            self.position += 1;
            Ok(token)
        } else if self.at(&TokenKind::Eof) {
            Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected },
                self.peek().span,
            ))
        } else {
            Err(self.unexpected(expected))
        }
    }

    fn expect_parameter(&mut self, expected: &'static str) -> Result<Identifier, ParseError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Parameter(value) => {
                self.position += 1;
                Ok(Identifier::new(value, token.span))
            }
            TokenKind::Eof => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected },
                token.span,
            )),
            _ => Err(self.unexpected_at(&token, expected)),
        }
    }

    fn expect_function_segment(
        &mut self,
        expected: &'static str,
    ) -> Result<Identifier, ParseError> {
        let token = self.peek().clone();
        let value =
            match function_segment_value(&token.kind).or_else(|| keyword_object_key(&token.kind)) {
                Some(value) => value,
                None if matches!(token.kind, TokenKind::Eof) => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedEof { expected },
                        token.span,
                    ));
                }
                None => return Err(self.unexpected(expected)),
            };
        self.position += 1;
        Ok(Identifier::new(value, token.span))
    }

    fn expect_path_segment(&mut self, expected: &'static str) -> Result<Identifier, ParseError> {
        let token = self.peek().clone();
        let value = match token.kind.clone() {
            TokenKind::Ident(value) => value,
            TokenKind::Eof => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedEof { expected },
                    token.span,
                ));
            }
            kind => match keyword_object_key(&kind) {
                Some(value) => value,
                None => return Err(self.unexpected_at(&token, expected)),
            },
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

    fn previous_span(&self) -> Span {
        self.tokens[self.position.saturating_sub(1)].span
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
        let _ = self;
        "AFTER, BEFORE, NONE, DIFF, or VALUE expression"
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

fn is_complex_record_id_expression(expression: &Expr) -> bool {
    match &expression.kind {
        ExprKind::None
        | ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Integer(_)
        | ExprKind::Float(_)
        | ExprKind::Duration(_)
        | ExprKind::String(_) => true,
        ExprKind::Array(values) => values.iter().all(is_complex_record_id_expression),
        ExprKind::Object(fields) => fields
            .iter()
            .all(|field| is_complex_record_id_expression(&field.value)),
        ExprKind::Unary { operator, operand } => {
            matches!(operator.value, UnaryOperator::Plus | UnaryOperator::Minus)
                && matches!(operand.kind, ExprKind::Integer(_) | ExprKind::Float(_))
        }
        ExprKind::Parenthesized(value) => is_complex_record_id_expression(value),
        _ => false,
    }
}

fn record_id_part_starts(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::Ident(_)
            | TokenKind::QuotedIdent(_)
            | TokenKind::Number(_)
            | TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::LeftBracket
            | TokenKind::LeftBrace
    ) || keyword_object_key(token).is_some()
}

fn keyword_object_key(token: &TokenKind) -> Option<String> {
    let description = token.describe();
    description
        .strip_prefix("keyword ")
        .or_else(|| description.strip_prefix("type "))
        .map(str::to_ascii_lowercase)
}

fn select_target_span(target: &SelectTarget) -> Span {
    match target {
        SelectTarget::Target(target) => target.span(),
        SelectTarget::Expression(expression) => expression.span,
        SelectTarget::Subquery(select) => select.span,
    }
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
        // v3.1.5 evaluates every other characterized binary operator before
        // either coalescing form (for example, `0 ?? 1 + 2` is `0`).
        TokenKind::NullCoalesce => (BinaryOperator::NullCoalesce, 0),
        TokenKind::TruthyCoalesce => (BinaryOperator::TruthyCoalesce, 0),
        TokenKind::Power => (BinaryOperator::Power, 13),
        TokenKind::Star => (BinaryOperator::Multiply, 11),
        TokenKind::Slash => (BinaryOperator::Divide, 11),
        TokenKind::Percent => (BinaryOperator::Modulo, 11),
        TokenKind::Plus => (BinaryOperator::Add, 9),
        TokenKind::Minus => (BinaryOperator::Subtract, 9),
        TokenKind::Contains => (BinaryOperator::Contains, 7),
        TokenKind::ContainsNot => (BinaryOperator::ContainsNot, 7),
        TokenKind::ContainsAll => (BinaryOperator::ContainsAll, 7),
        TokenKind::ContainsAny => (BinaryOperator::ContainsAny, 7),
        TokenKind::ContainsNone => (BinaryOperator::ContainsNone, 7),
        TokenKind::Inside | TokenKind::In => (BinaryOperator::Inside, 7),
        TokenKind::NotInside => (BinaryOperator::NotInside, 7),
        TokenKind::AllInside => (BinaryOperator::AllInside, 7),
        TokenKind::AnyInside => (BinaryOperator::AnyInside, 7),
        TokenKind::NoneInside => (BinaryOperator::NoneInside, 7),
        TokenKind::Less => (BinaryOperator::Less, 7),
        TokenKind::LessEqual => (BinaryOperator::LessEqual, 7),
        TokenKind::Greater => (BinaryOperator::Greater, 7),
        TokenKind::GreaterEqual => (BinaryOperator::GreaterEqual, 7),
        TokenKind::Equal => (BinaryOperator::Equal, 5),
        TokenKind::ExactEqual => (BinaryOperator::ExactEqual, 5),
        TokenKind::AnyEqual => (BinaryOperator::AnyEqual, 5),
        TokenKind::AllEqual => (BinaryOperator::AllEqual, 5),
        TokenKind::NotEqual => (BinaryOperator::NotEqual, 5),
        TokenKind::FtsMatch(reference) => (BinaryOperator::FtsMatch(*reference), 5),
        TokenKind::And => (BinaryOperator::And, 3),
        TokenKind::Or => (BinaryOperator::Or, 1),
        _ => return None,
    };
    Some((operator, power, power + 1))
}

fn function_segment_value(kind: &TokenKind) -> Option<String> {
    let value = match kind {
        TokenKind::Ident(value) => return Some(value.clone()),
        TokenKind::Search => "search",
        TokenKind::In => "in",
        TokenKind::Out => "out",
        TokenKind::Is => "is",
        TokenKind::Not => "not",
        TokenKind::None => "none",
        TokenKind::All => "all",
        TokenKind::Null => "null",
        TokenKind::Contains => "contains",
        TokenKind::Set => "set",
        TokenKind::Value => "value",
        TokenKind::Type => "type",
        TokenKind::Field => "field",
        TokenKind::Fields => "fields",
        TokenKind::Table => "table",
        TokenKind::Asc => "asc",
        TokenKind::Desc => "desc",
        TokenKind::Sleep => "sleep",
        TokenKind::Insert => "insert",
        TokenKind::Delete => "delete",
        TokenKind::Remove => "remove",
        TokenKind::Patch => "patch",
        TokenKind::Replace => "replace",
        TokenKind::Split => "split",
        TokenKind::Group => "group",
        TokenKind::Timeout => "timeout",
        TokenKind::Json => "json",
        TokenKind::BoolType => "bool",
        TokenKind::IntType => "int",
        TokenKind::FloatType => "float",
        TokenKind::NumberType => "number",
        TokenKind::DecimalType => "decimal",
        TokenKind::StringType => "string",
        TokenKind::BytesType => "bytes",
        TokenKind::DatetimeType => "datetime",
        TokenKind::DurationType => "duration",
        TokenKind::UuidType => "uuid",
        TokenKind::RegexType => "regex",
        TokenKind::FileType => "file",
        TokenKind::RangeType => "range",
        TokenKind::ObjectType => "object",
        TokenKind::ArrayType => "array",
        TokenKind::RecordType => "record",
        TokenKind::OptionType => "option",
        _ => return None,
    };
    Some(value.to_string())
}

fn excluded_operator_description(operator: &str) -> &'static str {
    match operator {
        "->" => "graph traversal is outside the MVP expression grammar",
        _ => "operator is outside the MVP expression grammar",
    }
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
            | TokenKind::Insert
            | TokenKind::Upsert
            | TokenKind::Delete
            | TokenKind::Define
            | TokenKind::Alter
            | TokenKind::Explain
            | TokenKind::Remove
            | TokenKind::Rebuild
            | TokenKind::Begin
            | TokenKind::Commit
            | TokenKind::Cancel
            | TokenKind::Let
            | TokenKind::Return
            | TokenKind::If
            | TokenKind::For
            | TokenKind::Break
            | TokenKind::Continue
            | TokenKind::Throw
            | TokenKind::Sleep
            | TokenKind::Info
    ) || is_unsupported_statement(kind)
}

fn is_unsupported_statement(kind: &TokenKind) -> bool {
    matches!(kind, TokenKind::Use | TokenKind::Live | TokenKind::Show)
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
