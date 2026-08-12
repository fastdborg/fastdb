//! Phase 0 lexer: hand-written, linear scan, no recursion.
//!
//! Tokens: keywords `CREATE SELECT FROM WHERE SET DELETE` (matched
//! case-insensitively), punctuation `: * . = , ;`, bare identifiers,
//! and single-quoted strings with `''` escaping. A backslash is a literal
//! backslash in Phase 0 (no C-style escapes).

use crate::ast::Span;
use crate::error::{ParseError, ParseErrorKind};

/// Hard cap on input size. Generous for Phase 0 forms; exists so the
/// parser can never be made to buffer unbounded input.
pub const MAX_INPUT_BYTES: usize = 1 << 20; // 1 MiB
/// Hard cap on token count, defending against pathologically dense input.
pub const MAX_TOKENS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Keywords (case-insensitive).
    Create,
    Select,
    From,
    Where,
    Set,
    Delete,
    // Punctuation / operators.
    Colon,
    Star,
    Dot,
    Eq,
    Comma,
    Semicolon,
    // Literals.
    Ident(String),
    String(String),
    Eof,
}

impl TokenKind {
    /// Human-readable description used in "expected X, found Y" messages.
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Create => "keyword CREATE".into(),
            TokenKind::Select => "keyword SELECT".into(),
            TokenKind::From => "keyword FROM".into(),
            TokenKind::Where => "keyword WHERE".into(),
            TokenKind::Set => "keyword SET".into(),
            TokenKind::Delete => "keyword DELETE".into(),
            TokenKind::Colon => "':'".into(),
            TokenKind::Star => "'*'".into(),
            TokenKind::Dot => "'.'".into(),
            TokenKind::Eq => "'='".into(),
            TokenKind::Comma => "','".into(),
            TokenKind::Semicolon => "';'".into(),
            TokenKind::Ident(s) => format!("identifier {s:?}"),
            TokenKind::String(_) => "string literal".into(),
            TokenKind::Eof => "end of input".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(ParseError::new(
            ParseErrorKind::LimitExceeded {
                what: "input bytes",
            },
            Span::new(0, input.len()),
        ));
    }
    let mut lex = Lexer::new(input);
    let mut tokens = Vec::new();
    loop {
        let tok = lex.next_token()?;
        let is_eof = matches!(tok.kind, TokenKind::Eof);
        tokens.push(tok);
        if is_eof {
            break;
        }
        if tokens.len() > MAX_TOKENS {
            return Err(ParseError::new(
                ParseErrorKind::LimitExceeded {
                    what: "token count",
                },
                Span::new(0, input.len()),
            ));
        }
    }
    Ok(tokens)
}

struct Lexer<'a> {
    chars: Vec<char>,
    /// Byte offset in the original input corresponding to `chars[self.pos]`.
    /// Stored in parallel so spans are byte-accurate.
    byte_offsets: Vec<usize>,
    pos: usize,
    input_len: usize,
    _src: &'a str,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        // Pre-compute char -> byte-offset map so spans are byte-accurate
        // while iteration works on chars.
        let mut chars = Vec::new();
        let mut byte_offsets = Vec::new();
        for (i, c) in src.char_indices() {
            chars.push(c);
            byte_offsets.push(i);
        }
        // Sentinel: byte offset of "one past the last char" for EOF spans.
        byte_offsets.push(src.len());
        Self {
            chars,
            byte_offsets,
            pos: 0,
            input_len: src.len(),
            _src: src,
        }
    }

    fn byte_offset(&self) -> usize {
        self.byte_offsets[self.pos.min(self.byte_offsets.len() - 1)]
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn next_token(&mut self) -> Result<Token, ParseError> {
        // Skip whitespace.
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
        let start = self.byte_offset();
        let Some(c) = self.peek() else {
            return Ok(Token::new(TokenKind::Eof, Span::new(self.input_len, 0)));
        };

        match c {
            ':' => {
                self.bump();
                Ok(Token::new(TokenKind::Colon, Span::new(start, 1)))
            }
            '*' => {
                self.bump();
                Ok(Token::new(TokenKind::Star, Span::new(start, 1)))
            }
            '.' => {
                self.bump();
                Ok(Token::new(TokenKind::Dot, Span::new(start, 1)))
            }
            '=' => {
                self.bump();
                Ok(Token::new(TokenKind::Eq, Span::new(start, 1)))
            }
            ',' => {
                self.bump();
                Ok(Token::new(TokenKind::Comma, Span::new(start, 1)))
            }
            ';' => {
                self.bump();
                Ok(Token::new(TokenKind::Semicolon, Span::new(start, 1)))
            }
            '\'' => self.lex_string(start),
            c if is_ident_start(c) => self.lex_ident(start),
            other => Err(ParseError::new(
                ParseErrorKind::UnexpectedChar { ch: other },
                Span::new(start, other.len_utf8()),
            )),
        }
    }

    fn lex_ident(&mut self, start: usize) -> Result<Token, ParseError> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        let end = self.byte_offset();
        let kind = classify_ident(&s);
        Ok(Token::new(kind, Span::new(start, end - start)))
    }

    fn lex_string(&mut self, start: usize) -> Result<Token, ParseError> {
        // Consume the opening quote.
        self.bump(); // '\''
        let mut value = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnterminatedString,
                        Span::new(start, self.byte_offset() - start),
                    ));
                }
                Some('\'') => {
                    // Doubled single quote => literal quote; otherwise close.
                    if self.peek2() == Some('\'') {
                        self.bump();
                        self.bump();
                        value.push('\'');
                    } else {
                        self.bump(); // closing quote
                        let end = self.byte_offset();
                        return Ok(Token::new(
                            TokenKind::String(value),
                            Span::new(start, end - start),
                        ));
                    }
                }
                Some(c) => {
                    value.push(c);
                    self.bump();
                }
            }
        }
    }
}

fn is_ident_start(c: char) -> bool {
    // Letters (Unicode) or underscore. Digits and '-' do not start an
    // identifier; '-' is not a Phase 0 token at all.
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Returns the keyword token kind if `s` is a Phase 0 keyword (matched
/// case-insensitively); otherwise an identifier carrying the original text.
fn classify_ident(s: &str) -> TokenKind {
    if s.eq_ignore_ascii_case("create") {
        TokenKind::Create
    } else if s.eq_ignore_ascii_case("select") {
        TokenKind::Select
    } else if s.eq_ignore_ascii_case("from") {
        TokenKind::From
    } else if s.eq_ignore_ascii_case("where") {
        TokenKind::Where
    } else if s.eq_ignore_ascii_case("set") {
        TokenKind::Set
    } else if s.eq_ignore_ascii_case("delete") {
        TokenKind::Delete
    } else {
        TokenKind::Ident(s.to_string())
    }
}
