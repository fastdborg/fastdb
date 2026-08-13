//! Linear UTF-8 lexer with byte-accurate spans.

use crate::ast::Span;
use crate::error::{LimitKind, ParseError, ParseErrorKind};
use crate::ParserLimits;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Create,
    Select,
    Update,
    Delete,
    Define,
    Alter,
    Table,
    Field,
    Index,
    Begin,
    Commit,
    Cancel,
    Only,
    Content,
    Set,
    Return,
    After,
    None,
    Before,
    From,
    Where,
    Order,
    By,
    Limit,
    Start,
    As,
    Asc,
    Desc,
    All,
    Collate,
    Numeric,
    Rand,
    At,
    On,
    Type,
    Fields,
    Unique,
    Schemaless,
    Schemafull,
    Normal,
    Relation,
    In,
    Out,
    To,
    Enforced,
    Null,
    True,
    False,
    Not,
    And,
    Or,
    Is,
    Contains,
    ContainsNot,
    ContainsAll,
    ContainsAny,
    ContainsNone,
    Inside,
    NotInside,
    AllInside,
    AnyInside,
    NoneInside,
    BoolType,
    IntType,
    FloatType,
    NumberType,
    DecimalType,
    StringType,
    BytesType,
    DatetimeType,
    DurationType,
    UuidType,
    RegexType,
    FileType,
    RangeType,
    ObjectType,
    ArrayType,
    RecordType,
    OptionType,
    Transaction,
    Insert,
    Upsert,
    Ignore,
    Into,
    Values,
    Duplicate,
    Key,
    Relate,
    Let,
    Remove,
    Rebuild,
    Info,
    Use,
    Live,
    Show,
    Sleep,
    Throw,
    For,
    If,
    Else,
    Break,
    Continue,
    Param,
    Exists,
    Overwrite,
    Timeout,
    Version,
    Fetch,
    Group,
    Split,
    Omit,
    Explain,
    Analyze,
    Full,
    Format,
    Json,
    With,
    Using,
    Value,
    Diff,
    Merge,
    Patch,
    Replace,
    Unset,
    Permissions,
    Assert,
    Default,
    Readonly,
    Changefeed,
    View,
    Fulltext,
    Search,
    Analyzer,
    Tokenizers,
    Highlights,
    Functions,
    Filters,
    Parallel,
    FtsMatch(Option<u32>),
    KnnStart,
    KnnEnd,
    ForwardArrow,
    ReverseArrow,
    BidirectionalArrow,
    DoubleColon,
    Colon,
    Star,
    Dot,
    Equal,
    ExactEqual,
    AnyEqual,
    AllEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Plus,
    Minus,
    Slash,
    Percent,
    Power,
    Range,
    RangeInclusive,
    NullCoalesce,
    TruthyCoalesce,
    Pipe,
    Dollar,
    Comma,
    Semicolon,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    UnsupportedOperator(&'static str),
    Ident(String),
    Parameter(String),
    Number(String),
    Duration(String),
    String(String),
    QuotedIdent(String),
    Eof,
}

impl TokenKind {
    pub fn describe(&self) -> String {
        match self {
            Self::Ident(value) => format!("identifier {value:?}"),
            Self::Parameter(value) => format!("parameter ${value}"),
            Self::Number(value) => format!("number {value:?}"),
            Self::Duration(value) => format!("duration {value:?}"),
            Self::String(_) => "string literal".into(),
            Self::QuotedIdent(_) => "backtick-quoted identifier".into(),
            Self::UnsupportedOperator(op) => format!("operator {op:?}"),
            Self::FtsMatch(None) => "operator @@".into(),
            Self::FtsMatch(Some(reference)) => format!("operator @{reference}@"),
            Self::Eof => "end of input".into(),
            other => other.fixed_description().into(),
        }
    }

    fn fixed_description(&self) -> &'static str {
        match self {
            Self::Create => "keyword CREATE",
            Self::Select => "keyword SELECT",
            Self::Update => "keyword UPDATE",
            Self::Delete => "keyword DELETE",
            Self::Define => "keyword DEFINE",
            Self::Alter => "keyword ALTER",
            Self::Table => "keyword TABLE",
            Self::Field => "keyword FIELD",
            Self::Index => "keyword INDEX",
            Self::Begin => "keyword BEGIN",
            Self::Commit => "keyword COMMIT",
            Self::Cancel => "keyword CANCEL",
            Self::Only => "keyword ONLY",
            Self::Content => "keyword CONTENT",
            Self::Set => "keyword SET",
            Self::Return => "keyword RETURN",
            Self::After => "keyword AFTER",
            Self::None => "keyword NONE",
            Self::Before => "keyword BEFORE",
            Self::From => "keyword FROM",
            Self::Where => "keyword WHERE",
            Self::Order => "keyword ORDER",
            Self::By => "keyword BY",
            Self::Limit => "keyword LIMIT",
            Self::Start => "keyword START",
            Self::As => "keyword AS",
            Self::Asc => "keyword ASC",
            Self::Desc => "keyword DESC",
            Self::All => "keyword ALL",
            Self::Collate => "keyword COLLATE",
            Self::Numeric => "keyword NUMERIC",
            Self::Rand => "keyword RAND",
            Self::At => "keyword AT",
            Self::On => "keyword ON",
            Self::Type => "keyword TYPE",
            Self::Fields => "keyword FIELDS",
            Self::Unique => "keyword UNIQUE",
            Self::Schemaless => "keyword SCHEMALESS",
            Self::Schemafull => "keyword SCHEMAFULL",
            Self::Normal => "keyword NORMAL",
            Self::Relation => "keyword RELATION",
            Self::In => "keyword IN",
            Self::Out => "keyword OUT",
            Self::To => "keyword TO",
            Self::Enforced => "keyword ENFORCED",
            Self::Null => "keyword NULL",
            Self::True => "keyword TRUE",
            Self::False => "keyword FALSE",
            Self::Not => "keyword NOT",
            Self::And => "keyword AND",
            Self::Or => "keyword OR",
            Self::Is => "keyword IS",
            Self::Contains => "keyword CONTAINS",
            Self::ContainsNot => "keyword CONTAINSNOT",
            Self::ContainsAll => "keyword CONTAINSALL",
            Self::ContainsAny => "keyword CONTAINSANY",
            Self::ContainsNone => "keyword CONTAINSNONE",
            Self::Inside => "keyword INSIDE",
            Self::NotInside => "keyword NOTINSIDE",
            Self::AllInside => "keyword ALLINSIDE",
            Self::AnyInside => "keyword ANYINSIDE",
            Self::NoneInside => "keyword NONEINSIDE",
            Self::BoolType => "type BOOL",
            Self::IntType => "type INT",
            Self::FloatType => "type FLOAT",
            Self::NumberType => "type NUMBER",
            Self::DecimalType => "type DECIMAL",
            Self::StringType => "type STRING",
            Self::BytesType => "type BYTES",
            Self::DatetimeType => "type DATETIME",
            Self::DurationType => "type DURATION",
            Self::UuidType => "type UUID",
            Self::RegexType => "type REGEX",
            Self::FileType => "type FILE",
            Self::RangeType => "type RANGE",
            Self::ObjectType => "type OBJECT",
            Self::ArrayType => "type ARRAY",
            Self::RecordType => "type RECORD",
            Self::OptionType => "type OPTION",
            Self::Transaction => "keyword TRANSACTION",
            Self::Insert => "keyword INSERT",
            Self::Upsert => "keyword UPSERT",
            Self::Ignore => "keyword IGNORE",
            Self::Into => "keyword INTO",
            Self::Values => "keyword VALUES",
            Self::Duplicate => "keyword DUPLICATE",
            Self::Key => "keyword KEY",
            Self::Relate => "keyword RELATE",
            Self::Let => "keyword LET",
            Self::Remove => "keyword REMOVE",
            Self::Rebuild => "keyword REBUILD",
            Self::Info => "keyword INFO",
            Self::Use => "keyword USE",
            Self::Live => "keyword LIVE",
            Self::Show => "keyword SHOW",
            Self::Sleep => "keyword SLEEP",
            Self::Throw => "keyword THROW",
            Self::For => "keyword FOR",
            Self::If => "keyword IF",
            Self::Else => "keyword ELSE",
            Self::Break => "keyword BREAK",
            Self::Continue => "keyword CONTINUE",
            Self::Param => "keyword PARAM",
            Self::Exists => "keyword EXISTS",
            Self::Overwrite => "keyword OVERWRITE",
            Self::Timeout => "keyword TIMEOUT",
            Self::Version => "keyword VERSION",
            Self::Fetch => "keyword FETCH",
            Self::Group => "keyword GROUP",
            Self::Split => "keyword SPLIT",
            Self::Omit => "keyword OMIT",
            Self::Explain => "keyword EXPLAIN",
            Self::Analyze => "keyword ANALYZE",
            Self::Full => "keyword FULL",
            Self::Format => "keyword FORMAT",
            Self::Json => "keyword JSON",
            Self::With => "keyword WITH",
            Self::Using => "keyword USING",
            Self::Value => "keyword VALUE",
            Self::Diff => "keyword DIFF",
            Self::Merge => "keyword MERGE",
            Self::Patch => "keyword PATCH",
            Self::Replace => "keyword REPLACE",
            Self::Unset => "keyword UNSET",
            Self::Permissions => "keyword PERMISSIONS",
            Self::Assert => "keyword ASSERT",
            Self::Default => "keyword DEFAULT",
            Self::Readonly => "keyword READONLY",
            Self::Changefeed => "keyword CHANGEFEED",
            Self::View => "keyword VIEW",
            Self::Fulltext => "keyword FULLTEXT",
            Self::Search => "keyword SEARCH",
            Self::Analyzer => "keyword ANALYZER",
            Self::Tokenizers => "keyword TOKENIZERS",
            Self::Highlights => "keyword HIGHLIGHTS",
            Self::Functions => "keyword FUNCTIONS",
            Self::Filters => "keyword FILTERS",
            Self::Parallel => "keyword PARALLEL",
            Self::KnnStart => "'<|'",
            Self::KnnEnd => "'|>'",
            Self::ForwardArrow => "'->'",
            Self::ReverseArrow => "'<-'",
            Self::BidirectionalArrow => "'<->'",
            Self::DoubleColon => "'::'",
            Self::Colon => "':'",
            Self::Star => "'*'",
            Self::Dot => "'.'",
            Self::Equal => "'='",
            Self::ExactEqual => "'=='",
            Self::AnyEqual => "'?='",
            Self::AllEqual => "'*='",
            Self::NotEqual => "'!='",
            Self::Less => "'<'",
            Self::LessEqual => "'<='",
            Self::Greater => "'>'",
            Self::GreaterEqual => "'>='",
            Self::Plus => "'+'",
            Self::Minus => "'-'",
            Self::Slash => "'/'",
            Self::Percent => "'%'",
            Self::Power => "'**'",
            Self::Range => "'..'",
            Self::RangeInclusive => "'..='",
            Self::NullCoalesce => "'??'",
            Self::TruthyCoalesce => "'?:'",
            Self::Pipe => "'|'",
            Self::Dollar => "'$'",
            Self::Comma => "','",
            Self::Semicolon => "';'",
            Self::LeftParen => "'('",
            Self::RightParen => "')'",
            Self::LeftBracket => "'['",
            Self::RightBracket => "']'",
            Self::LeftBrace => "'{'",
            Self::RightBrace => "'}'",
            Self::UnsupportedOperator(_)
            | Self::FtsMatch(_)
            | Self::Ident(_)
            | Self::Parameter(_) => unreachable!(),
            Self::Number(_)
            | Self::Duration(_)
            | Self::String(_)
            | Self::QuotedIdent(_)
            | Self::Eof => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    tokenize_with_limits(input, &ParserLimits::default())
}

pub fn tokenize_with_limits(input: &str, limits: &ParserLimits) -> Result<Vec<Token>, ParseError> {
    if input.len() > limits.max_input_bytes {
        return Err(ParseError::new(
            ParseErrorKind::LimitExceeded {
                kind: LimitKind::InputBytes,
                limit: limits.max_input_bytes,
            },
            Span::new(limits.max_input_bytes, input.len() - limits.max_input_bytes),
        ));
    }

    let mut lexer = Lexer {
        source: input,
        position: 0,
        limits,
    };
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token()?;
        let eof = matches!(token.kind, TokenKind::Eof);
        if !eof && tokens.len() == limits.max_tokens {
            return Err(ParseError::new(
                ParseErrorKind::LimitExceeded {
                    kind: LimitKind::Tokens,
                    limit: limits.max_tokens,
                },
                token.span,
            ));
        }
        tokens.push(token);
        if eof {
            return Ok(tokens);
        }
    }
}

pub(crate) struct Lexer<'a> {
    pub(crate) source: &'a str,
    pub(crate) position: usize,
    pub(crate) limits: &'a ParserLimits,
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.source[self.position..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut chars = self.source[self.position..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.position += value.len_utf8();
        Some(value)
    }

    fn starts_with(&self, value: &str) -> bool {
        self.source[self.position..].starts_with(value)
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        self.skip_trivia()?;
        let start = self.position;
        let Some(ch) = self.peek() else {
            return Ok(Token::new(TokenKind::Eof, Span::new(start, 0)));
        };

        let single = |kind| Ok(Token::new(kind, Span::new(start, 1)));
        match ch {
            ':' if self.peek_next() == Some(':') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::DoubleColon, Span::new(start, 2)))
            }
            ':' => {
                self.bump();
                single(TokenKind::Colon)
            }
            ',' => {
                self.bump();
                single(TokenKind::Comma)
            }
            ';' => {
                self.bump();
                single(TokenKind::Semicolon)
            }
            '(' => {
                self.bump();
                single(TokenKind::LeftParen)
            }
            ')' => {
                self.bump();
                single(TokenKind::RightParen)
            }
            '[' => {
                self.bump();
                single(TokenKind::LeftBracket)
            }
            ']' => {
                self.bump();
                single(TokenKind::RightBracket)
            }
            '{' => {
                self.bump();
                single(TokenKind::LeftBrace)
            }
            '}' => {
                self.bump();
                single(TokenKind::RightBrace)
            }
            '+' => {
                self.bump();
                single(TokenKind::Plus)
            }
            '-' if self.peek_next() == Some('>') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::ForwardArrow, Span::new(start, 2)))
            }
            '-' => {
                self.bump();
                single(TokenKind::Minus)
            }
            '*' if self.peek_next() == Some('*') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::Power, Span::new(start, 2)))
            }
            '*' if self.peek_next() == Some('=') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::AllEqual, Span::new(start, 2)))
            }
            '*' => {
                self.bump();
                single(TokenKind::Star)
            }
            '/' => {
                self.bump();
                single(TokenKind::Slash)
            }
            '.' if self.starts_with("..=") => {
                self.bump();
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::RangeInclusive, Span::new(start, 3)))
            }
            '.' if self.peek_next() == Some('.') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::Range, Span::new(start, 2)))
            }
            '.' => {
                self.bump();
                single(TokenKind::Dot)
            }
            '=' if self.peek_next() == Some('=') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::ExactEqual, Span::new(start, 2)))
            }
            '=' => {
                self.bump();
                single(TokenKind::Equal)
            }
            '!' if self.peek_next() == Some('=') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::NotEqual, Span::new(start, 2)))
            }
            '!' => {
                self.bump();
                single(TokenKind::Not)
            }
            '<' if self.starts_with("<->") => {
                self.bump();
                self.bump();
                self.bump();
                Ok(Token::new(
                    TokenKind::BidirectionalArrow,
                    Span::new(start, 3),
                ))
            }
            '<' if self.peek_next() == Some('-') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::ReverseArrow, Span::new(start, 2)))
            }
            '<' if self.peek_next() == Some('=') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::LessEqual, Span::new(start, 2)))
            }
            '<' if self.peek_next() == Some('|') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::KnnStart, Span::new(start, 2)))
            }
            '<' => {
                self.bump();
                single(TokenKind::Less)
            }
            '>' if self.peek_next() == Some('=') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::GreaterEqual, Span::new(start, 2)))
            }
            '>' => {
                self.bump();
                single(TokenKind::Greater)
            }
            '|' if self.peek_next() == Some('>') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::KnnEnd, Span::new(start, 2)))
            }
            '@' => self.lex_fts_match(start),
            '%' => {
                self.bump();
                single(TokenKind::Percent)
            }
            '?' if self.peek_next() == Some('=') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::AnyEqual, Span::new(start, 2)))
            }
            '?' if self.peek_next() == Some('?') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::NullCoalesce, Span::new(start, 2)))
            }
            '?' if self.peek_next() == Some(':') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::TruthyCoalesce, Span::new(start, 2)))
            }
            '&' if self.peek_next() == Some('&') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::And, Span::new(start, 2)))
            }
            '|' if self.peek_next() == Some('|') => {
                self.bump();
                self.bump();
                Ok(Token::new(TokenKind::Or, Span::new(start, 2)))
            }
            '|' => {
                self.bump();
                single(TokenKind::Pipe)
            }
            '$' if self.peek_next().is_some_and(is_identifier_start) => self.lex_parameter(start),
            '$' => {
                self.bump();
                single(TokenKind::Dollar)
            }
            '×' => {
                self.bump();
                Ok(Token::new(
                    TokenKind::Star,
                    Span::new(start, '×'.len_utf8()),
                ))
            }
            '÷' => {
                self.bump();
                Ok(Token::new(
                    TokenKind::Slash,
                    Span::new(start, '÷'.len_utf8()),
                ))
            }
            '∋' => self.lex_unicode_operator(start, TokenKind::Contains),
            '∌' => self.lex_unicode_operator(start, TokenKind::ContainsNot),
            '∈' => self.lex_unicode_operator(start, TokenKind::Inside),
            '∉' => self.lex_unicode_operator(start, TokenKind::NotInside),
            '⊇' => self.lex_unicode_operator(start, TokenKind::ContainsAll),
            '⊃' => self.lex_unicode_operator(start, TokenKind::ContainsAny),
            '⊅' => self.lex_unicode_operator(start, TokenKind::ContainsNone),
            '⊆' => self.lex_unicode_operator(start, TokenKind::AllInside),
            '⊂' => self.lex_unicode_operator(start, TokenKind::AnyInside),
            '⊄' => self.lex_unicode_operator(start, TokenKind::NoneInside),
            '\'' | '"' => self.lex_string(start, ch),
            '`' => self.lex_quoted_identifier(start),
            value if value.is_ascii_digit() => self.lex_number(start),
            value if is_identifier_start(value) => self.lex_identifier(start),
            other => Err(ParseError::new(
                ParseErrorKind::UnexpectedCharacter { ch: other },
                Span::new(start, other.len_utf8()),
            )),
        }
    }

    fn skip_trivia(&mut self) -> Result<(), ParseError> {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.bump();
            }
            if self.starts_with("#") || self.starts_with("//") || self.starts_with("--") {
                while let Some(ch) = self.bump() {
                    if ch == '\n' {
                        break;
                    }
                }
                continue;
            }
            if self.starts_with("/*") {
                let start = self.position;
                self.position += 2;
                while !self.starts_with("*/") {
                    if self.bump().is_none() {
                        return Err(ParseError::new(
                            ParseErrorKind::UnterminatedComment,
                            Span::new(start, self.position - start),
                        ));
                    }
                }
                self.position += 2;
                continue;
            }
            return Ok(());
        }
    }

    fn lex_unicode_operator(&mut self, start: usize, kind: TokenKind) -> Result<Token, ParseError> {
        let width = self.bump().expect("operator is present").len_utf8();
        Ok(Token::new(kind, Span::new(start, width)))
    }

    fn lex_identifier(&mut self, start: usize) -> Result<Token, ParseError> {
        self.bump();
        while self.peek().is_some_and(is_identifier_continue) {
            self.bump();
        }
        let text = &self.source[start..self.position];
        self.check_identifier_limit(text, Span::new(start, text.len()))?;
        Ok(Token::new(
            classify_identifier(text),
            Span::new(start, text.len()),
        ))
    }

    fn lex_parameter(&mut self, start: usize) -> Result<Token, ParseError> {
        self.bump();
        let name_start = self.position;
        let Some(first) = self.peek() else {
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: "a parameter name after '$'",
                },
                Span::new(self.position, 0),
            ));
        };
        if !is_identifier_start(first) {
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedCharacter { ch: first },
                Span::new(self.position, first.len_utf8()),
            ));
        }
        self.bump();
        while self.peek().is_some_and(is_identifier_continue) {
            self.bump();
        }
        let name = &self.source[name_start..self.position];
        self.check_identifier_limit(name, Span::new(name_start, name.len()))?;
        Ok(Token::new(
            TokenKind::Parameter(name.to_string()),
            Span::new(start, self.position - start),
        ))
    }

    fn lex_fts_match(&mut self, start: usize) -> Result<Token, ParseError> {
        self.bump();
        if self.peek().is_none() {
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedCharacter { ch: '@' },
                Span::new(start, 1),
            ));
        }
        if self.peek() == Some('@') {
            self.bump();
            return Ok(Token::new(TokenKind::FtsMatch(None), Span::new(start, 2)));
        }

        let digits_start = self.position;
        while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            self.bump();
        }
        if self.position == digits_start || self.peek() != Some('@') {
            while self
                .peek()
                .is_some_and(|ch| !ch.is_whitespace() && !matches!(ch, ';' | ',' | ')' | ']'))
            {
                self.bump();
            }
            return Err(ParseError::unsupported(
                "only @@ and @<digits>@ FTS match operators are supported",
                Span::new(start, self.position - start),
            ));
        }
        let reference = self.source[digits_start..self.position]
            .parse::<u32>()
            .map_err(|_| {
                ParseError::unsupported(
                    "FTS match references must fit an unsigned 32-bit integer",
                    Span::new(start, self.position - start + 1),
                )
            })?;
        self.bump();
        Ok(Token::new(
            TokenKind::FtsMatch(Some(reference)),
            Span::new(start, self.position - start),
        ))
    }

    fn lex_number(&mut self, start: usize) -> Result<Token, ParseError> {
        while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            self.bump();
        }
        if self.peek() == Some('.') && self.peek_next().is_some_and(|ch| ch.is_ascii_digit()) {
            self.bump();
            while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            let exponent_start = self.position;
            while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                self.bump();
            }
            if self.position == exponent_start {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidNumber {
                        literal: self.source[start..self.position].to_string(),
                        reason: "exponent requires at least one digit",
                    },
                    Span::new(start, self.position - start),
                ));
            }
        }
        if self.peek().is_some_and(is_identifier_start) {
            let suffix_start = self.position;
            while self.peek().is_some_and(char::is_alphabetic) {
                self.bump();
            }
            let suffix = &self.source[suffix_start..self.position];
            if is_duration_suffix(suffix) {
                while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                    while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                        self.bump();
                    }
                    let suffix_start = self.position;
                    while self.peek().is_some_and(char::is_alphabetic) {
                        self.bump();
                    }
                    let suffix = &self.source[suffix_start..self.position];
                    if !is_duration_suffix(suffix) {
                        return Err(ParseError::new(
                            ParseErrorKind::InvalidNumber {
                                literal: self.source[start..self.position].to_string(),
                                reason: "duration component has an unknown unit",
                            },
                            Span::new(start, self.position - start),
                        ));
                    }
                }
                return Ok(Token::new(
                    TokenKind::Duration(self.source[start..self.position].to_string()),
                    Span::new(start, self.position - start),
                ));
            }
            return Err(ParseError::new(
                ParseErrorKind::InvalidNumber {
                    literal: self.source[start..self.position].to_string(),
                    reason: "number must be separated from following text",
                },
                Span::new(start, self.position - start),
            ));
        }
        let value = self.source[start..self.position].to_string();
        Ok(Token::new(
            TokenKind::Number(value),
            Span::new(start, self.position - start),
        ))
    }

    fn lex_string(&mut self, start: usize, delimiter: char) -> Result<Token, ParseError> {
        self.bump();
        let mut value = String::new();
        loop {
            let Some(ch) = self.bump() else {
                return Err(ParseError::new(
                    ParseErrorKind::UnterminatedString { delimiter },
                    Span::new(start, self.position - start),
                ));
            };
            if ch == delimiter {
                if self.peek() == Some(delimiter) {
                    self.bump();
                    value.push(delimiter);
                    continue;
                }
                return Ok(Token::new(
                    TokenKind::String(value),
                    Span::new(start, self.position - start),
                ));
            }
            if ch != '\\' {
                value.push(ch);
                continue;
            }
            let escape_start = self.position - 1;
            let Some(escaped) = self.bump() else {
                return Err(ParseError::new(
                    ParseErrorKind::UnterminatedString { delimiter },
                    Span::new(start, self.position - start),
                ));
            };
            match escaped {
                '\\' => value.push('\\'),
                '\'' => value.push('\''),
                '"' => value.push('"'),
                'b' => value.push('\u{0008}'),
                'f' => value.push('\u{000c}'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                'u' => value.push(self.lex_unicode_escape(escape_start)?),
                other => {
                    return Err(ParseError::new(
                        ParseErrorKind::InvalidEscape {
                            escape: format!("\\{other}"),
                        },
                        Span::new(escape_start, self.position - escape_start),
                    ));
                }
            }
        }
    }

    fn lex_unicode_escape(&mut self, escape_start: usize) -> Result<char, ParseError> {
        let digits_start = self.position;
        for _ in 0..4 {
            if self.peek().is_some_and(|ch| ch.is_ascii_hexdigit()) {
                self.bump();
            } else {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidEscape {
                        escape: self.source[escape_start..self.position].to_string(),
                    },
                    Span::new(escape_start, self.position - escape_start),
                ));
            }
        }
        let digits = &self.source[digits_start..self.position];
        let scalar = u32::from_str_radix(digits, 16).expect("four hex digits fit u32");
        char::from_u32(scalar).ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::InvalidEscape {
                    escape: self.source[escape_start..self.position].to_string(),
                },
                Span::new(escape_start, self.position - escape_start),
            )
        })
    }

    fn lex_quoted_identifier(&mut self, start: usize) -> Result<Token, ParseError> {
        self.bump();
        let mut value = String::new();
        loop {
            let Some(ch) = self.bump() else {
                return Err(ParseError::new(
                    ParseErrorKind::UnterminatedQuotedIdentifier,
                    Span::new(start, self.position - start),
                ));
            };
            if ch == '`' {
                if self.peek() == Some('`') {
                    self.bump();
                    value.push('`');
                    continue;
                }
                self.check_identifier_limit(&value, Span::new(start, self.position - start))?;
                return Ok(Token::new(
                    TokenKind::QuotedIdent(value),
                    Span::new(start, self.position - start),
                ));
            }
            if ch == '\\' && self.peek() == Some('`') {
                self.bump();
                value.push('`');
            } else {
                value.push(ch);
            }
        }
    }

    fn check_identifier_limit(&self, value: &str, span: Span) -> Result<(), ParseError> {
        if value.len() <= self.limits.max_identifier_bytes {
            return Ok(());
        }
        Err(ParseError::new(
            ParseErrorKind::LimitExceeded {
                kind: LimitKind::IdentifierBytes,
                limit: self.limits.max_identifier_bytes,
            },
            span,
        ))
    }
}

fn is_duration_suffix(value: &str) -> bool {
    matches!(
        value,
        "ns" | "us" | "µs" | "ms" | "s" | "m" | "h" | "d" | "w" | "y"
    )
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

fn classify_identifier(value: &str) -> TokenKind {
    macro_rules! keyword {
        ($($text:literal => $kind:ident),+ $(,)?) => {
            $(if value.eq_ignore_ascii_case($text) { return TokenKind::$kind; })+
        };
    }
    keyword! {
        "create" => Create, "select" => Select, "update" => Update, "delete" => Delete,
        "define" => Define, "alter" => Alter, "table" => Table, "field" => Field, "index" => Index,
        "begin" => Begin, "commit" => Commit, "cancel" => Cancel, "only" => Only,
        "content" => Content, "set" => Set, "return" => Return, "after" => After,
        "none" => None, "before" => Before, "from" => From, "where" => Where,
        "order" => Order, "by" => By, "limit" => Limit, "start" => Start,
        "as" => As, "asc" => Asc, "desc" => Desc, "all" => All,
        "collate" => Collate, "numeric" => Numeric, "rand" => Rand, "at" => At,
        "on" => On, "type" => Type,
        "fields" => Fields, "unique" => Unique, "schemaless" => Schemaless,
        "schemafull" => Schemafull, "normal" => Normal, "relation" => Relation,
        "in" => In, "out" => Out, "to" => To, "enforced" => Enforced,
        "null" => Null, "true" => True, "false" => False,
        "not" => Not, "and" => And, "or" => Or, "is" => Is,
        "contains" => Contains, "containsnot" => ContainsNot,
        "containsall" => ContainsAll, "containsany" => ContainsAny,
        "containsnone" => ContainsNone, "inside" => Inside,
        "notinside" => NotInside, "allinside" => AllInside,
        "anyinside" => AnyInside, "noneinside" => NoneInside,
        "bool" => BoolType, "int" => IntType,
        "float" => FloatType, "number" => NumberType, "string" => StringType,
        "decimal" => DecimalType, "bytes" => BytesType, "datetime" => DatetimeType,
        "duration" => DurationType, "uuid" => UuidType, "regex" => RegexType,
        "file" => FileType, "range" => RangeType,
        "object" => ObjectType, "array" => ArrayType, "record" => RecordType,
        "option" => OptionType, "transaction" => Transaction, "insert" => Insert,
        "upsert" => Upsert, "ignore" => Ignore, "into" => Into, "values" => Values,
        "duplicate" => Duplicate, "key" => Key,
        "relate" => Relate, "let" => Let, "remove" => Remove,
        "rebuild" => Rebuild,
        "info" => Info, "use" => Use, "live" => Live, "show" => Show, "sleep" => Sleep,
        "throw" => Throw, "for" => For, "if" => If, "else" => Else,
        "break" => Break, "continue" => Continue, "param" => Param,
        "exists" => Exists, "overwrite" => Overwrite, "timeout" => Timeout, "version" => Version,
        "fetch" => Fetch, "group" => Group, "split" => Split, "omit" => Omit,
        "explain" => Explain, "analyze" => Analyze, "full" => Full,
        "format" => Format, "json" => Json,
        "with" => With, "using" => Using, "value" => Value,
        "diff" => Diff,
        "merge" => Merge,
        "patch" => Patch, "replace" => Replace, "unset" => Unset,
        "permissions" => Permissions, "assert" => Assert, "default" => Default,
        "readonly" => Readonly, "changefeed" => Changefeed, "view" => View,
        "fulltext" => Fulltext, "search" => Search, "analyzer" => Analyzer,
        "tokenizers" => Tokenizers, "highlights" => Highlights,
        "functions" => Functions, "filters" => Filters,
        "parallel" => Parallel,
    }
    TokenKind::Ident(value.to_string())
}
