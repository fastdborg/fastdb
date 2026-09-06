//! Additive FastQL dispatch. Unrecognized statements retain their original SQL.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
#[error("FastQL syntax at byte {offset}: {message}")]
pub struct Error {
    pub offset: usize,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Key {
    Integer(i64),
    String(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub table: String,
    pub key: Key,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Null,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),
    Record(Record),
    Parameter(String),
    Object(BTreeMap<String, Expr>),
    Array(Vec<Expr>),
}
#[derive(Debug, PartialEq)]
pub enum Statement {
    Upsert {
        table: String,
        target: Option<Record>,
        value: Expr,
        returning: bool,
    },
    RemoveField {
        table: String,
        path: Vec<String>,
    },
    Info {
        scope: String,
        name: Option<String>,
    },
    DefineField {
        table: String,
        path: Vec<String>,
        kind: String,
        target: Option<String>,
        required: bool,
        nullable: bool,
        overwrite: bool,
    },
    CreateIndex {
        if_not_exists: bool,
        table: String,
        name: String,
        path: Vec<String>,
        unique: bool,
        sql: String,
    },
    Sql(String),
    CreateCollection {
        name: String,
        if_not_exists: bool,
    },
    Insert {
        table: String,
        value: Expr,
        returning: bool,
    },
    SelectRecord(Record),
    Patch {
        target: Record,
        value: Expr,
        returning: bool,
    },
    Delete {
        target: Record,
        returning: bool,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Word,
    Identifier,
    String,
    Parameter,
    Number,
    Symbol,
}
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// SQL-aware tokenization used for extension dispatch and managed-name checks.
pub fn tokenize(input: &str) -> Result<Vec<Token>> {
    let b = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut object_depth: usize = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b[i..].starts_with(b"--") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b[i..].starts_with(b"/*") {
            let start = i;
            i += 2;
            while i + 1 < b.len() && !b[i..].starts_with(b"*/") {
                i += 1;
            }
            if i + 1 == b.len() || i == b.len() {
                return Err(Error {
                    offset: start,
                    message: "unterminated comment".into(),
                });
            }
            i += 2;
            continue;
        }
        let start = i;
        let (kind, text) = match b[i] {
            b'[' if object_depth > 0 => {
                i += 1;
                (Kind::Symbol, "[".into())
            }
            b'\'' | b'"' | b'`' | b'[' => {
                let open = b[i];
                let close = if open == b'[' { b']' } else { open };
                i += 1;
                let mut text = String::new();
                let mut part = i;
                loop {
                    if i == b.len() {
                        return Err(Error {
                            offset: start,
                            message: "unterminated quote".into(),
                        });
                    }
                    if b[i] == close {
                        text.push_str(&input[part..i]);
                        i += 1;
                        if open != b'[' && i < b.len() && b[i] == close {
                            text.push(close as char);
                            i += 1;
                            part = i;
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                (
                    if open == b'\'' {
                        Kind::String
                    } else {
                        Kind::Identifier
                    },
                    text,
                )
            }
            b'$' | b'@' | b'?' => {
                i += 1;
                while i < b.len()
                    && (b[i].is_ascii_alphanumeric()
                        || b[i] == b'_'
                        || b[i] >= 128
                        || (b[start] == b'$' && b[i] == b':'))
                {
                    i += 1;
                }
                (Kind::Parameter, input[start..i].to_owned())
            }
            c if c.is_ascii_digit() => {
                i += 1;
                while i < b.len()
                    && (b[i].is_ascii_digit()
                        || matches!(b[i], b'.' | b'e' | b'E')
                        || (matches!(b[i], b'+' | b'-') && matches!(b[i - 1], b'e' | b'E')))
                {
                    i += 1;
                }
                (Kind::Number, input[start..i].to_owned())
            }
            c if c.is_ascii_alphabetic() || c == b'_' || c >= 128 => {
                i += 1;
                while i < b.len()
                    && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'$') || b[i] >= 128)
                {
                    i += 1;
                }
                (Kind::Word, input[start..i].to_owned())
            }
            _ => {
                i += 1;
                (Kind::Symbol, input[start..i].to_owned())
            }
        };
        if kind == Kind::Symbol {
            if text == "{" {
                object_depth += 1;
            }
            if text == "}" {
                object_depth = object_depth.saturating_sub(1);
            }
        }
        out.push(Token {
            kind,
            text,
            start,
            end: i,
        });
    }
    Ok(out)
}
struct Parser<'a> {
    input: &'a str,
    tokens: Vec<Token>,
    pos: usize,
}
impl Parser<'_> {
    fn error(&self, message: &str) -> Error {
        Error {
            offset: self
                .tokens
                .get(self.pos)
                .map_or(self.input.len(), |t| t.start),
            message: message.into(),
        }
    }
    fn eat(&mut self, text: &str) -> bool {
        if self.tokens.get(self.pos).is_some_and(|t| {
            matches!(t.kind, Kind::Word | Kind::Symbol) && t.text.eq_ignore_ascii_case(text)
        }) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn name(&mut self) -> Result<String> {
        let t = self
            .tokens
            .get(self.pos)
            .ok_or_else(|| self.error("expected name"))?;
        if !matches!(t.kind, Kind::Word | Kind::Identifier) {
            return Err(self.error("expected name"));
        }
        let name = t.text.clone();
        self.pos += 1;
        Ok(name)
    }
    fn end(&mut self) -> bool {
        self.eat(";");
        self.pos == self.tokens.len()
    }
    fn returning(&mut self) -> Result<bool> {
        let returning = self.eat("RETURNING");
        if returning && !self.eat("*") {
            return Err(self.error("expected RETURNING *"));
        }
        if !self.end() {
            return Err(self.error("unexpected trailing syntax"));
        }
        Ok(returning)
    }
    fn path(&mut self) -> Result<Vec<String>> {
        let mut path = vec![self.name()?];
        while self.eat(".") {
            path.push(self.name()?);
        }
        Ok(path)
    }
    fn record(&mut self) -> Result<Record> {
        let table_pos = self.pos;
        let table = self.name()?;
        let table_token = &self.tokens[table_pos];
        if table_token.kind == Kind::Identifier && self.input.as_bytes()[table_token.start] != b'`'
        {
            return Err(self.error("record targets use bare or backtick names"));
        }
        let end = table_token.end;
        if self.tokens.get(self.pos).is_none_or(|t| t.start != end) || !self.eat(":") {
            return Err(self.error("expected adjacent record colon"));
        }
        let colon_end = self.tokens[self.pos - 1].end;
        let t = self
            .tokens
            .get(self.pos)
            .ok_or_else(|| self.error("expected record key"))?
            .clone();
        if t.start != colon_end {
            return Err(self.error("record key must touch colon"));
        }
        let negative = self.eat("-");
        let positive = !negative && self.eat("+");
        let t = self
            .tokens
            .get(self.pos)
            .ok_or_else(|| self.error("expected key"))?
            .clone();
        if (negative || positive) && t.start != colon_end + 1 {
            return Err(self.error("expected adjacent integer key"));
        }
        let key = match t.kind {
            Kind::Number => {
                let s = if negative {
                    format!("-{}", t.text)
                } else {
                    t.text.clone()
                };
                Key::Integer(
                    s.parse()
                        .map_err(|_| self.error("record integer key must fit int64"))?,
                )
            }
            Kind::Word
                if !negative
                    && !positive
                    && t.text
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_') =>
            {
                Key::String(t.text)
            }
            Kind::Identifier
                if !negative
                    && !positive
                    && self.input.as_bytes()[t.start] == b'`'
                    && !t.text.is_empty() =>
            {
                Key::String(t.text)
            }
            _ => return Err(self.error("invalid record key")),
        };
        self.pos += 1;
        Ok(Record {
            table: table.to_ascii_lowercase(),
            key,
        })
    }
    fn expr(&mut self, depth: usize) -> Result<Expr> {
        if depth > 64 {
            return Err(self.error("document nesting limit exceeded"));
        }
        if self.eat("{") {
            let mut fields = BTreeMap::new();
            if self.eat("}") {
                return Ok(Expr::Object(fields));
            }
            loop {
                let t = self
                    .tokens
                    .get(self.pos)
                    .ok_or_else(|| self.error("expected object key"))?;
                if t.kind == Kind::Identifier && self.input.as_bytes()[t.start] != b'"' {
                    return Err(self.error("object keys use bare or double-quoted names"));
                }
                let name = self.name()?;
                if !self.eat(":") {
                    return Err(self.error("expected object colon"));
                }
                let value = self.expr(depth + 1)?;
                if fields.insert(name, value).is_some() {
                    return Err(self.error("duplicate object key"));
                }
                if self.eat("}") {
                    break;
                }
                if !self.eat(",") {
                    return Err(self.error("expected comma"));
                }
                if self.eat("}") {
                    break;
                }
            }
            return Ok(Expr::Object(fields));
        }
        if self.eat("[") {
            let mut values = Vec::new();
            if self.eat("]") {
                return Ok(Expr::Array(values));
            }
            loop {
                values.push(self.expr(depth + 1)?);
                if self.eat("]") {
                    break;
                }
                if !self.eat(",") {
                    return Err(self.error("expected array comma"));
                }
                if self.eat("]") {
                    break;
                }
            }
            return Ok(Expr::Array(values));
        }
        let t = self
            .tokens
            .get(self.pos)
            .ok_or_else(|| self.error("expected value"))?
            .clone();
        if matches!(t.kind, Kind::Word | Kind::Identifier)
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|next| next.text == ":" && next.start == t.end)
        {
            return self.record().map(Expr::Record);
        }
        if self.eat("NULL") {
            return Ok(Expr::Null);
        }
        if self.eat("TRUE") {
            return Ok(Expr::Boolean(true));
        }
        if self.eat("FALSE") {
            return Ok(Expr::Boolean(false));
        }
        let negative = self.eat("-");
        let t = self
            .tokens
            .get(self.pos)
            .ok_or_else(|| self.error("expected value"))?
            .clone();
        self.pos += 1;
        match t.kind {
            Kind::String if !negative => Ok(Expr::String(t.text)),
            Kind::Parameter if !negative => Ok(Expr::Parameter(t.text)),
            Kind::Number => {
                let text = if negative {
                    format!("-{}", t.text)
                } else {
                    t.text
                };
                if !text.contains(['.', 'e', 'E']) {
                    return text
                        .parse()
                        .map(Expr::Integer)
                        .map_err(|_| self.error("integer must fit int64"));
                }
                let n: f64 = text.parse().map_err(|_| self.error("invalid number"))?;
                if !n.is_finite() {
                    return Err(self.error("number must be finite"));
                }
                Ok(Expr::Number(n))
            }
            _ => Err(self.error("unsupported document expression")),
        }
    }
}
pub fn parse(input: &str) -> Result<Statement> {
    let mut p = Parser {
        input,
        tokens: tokenize(input)?,
        pos: 0,
    };
    if p.eat("UPSERT") {
        let target = if p.tokens.get(p.pos + 1).is_some_and(|t| t.text == ":") {
            Some(p.record()?)
        } else {
            None
        };
        let table = match &target {
            Some(r) => r.table.clone(),
            None => p.name()?,
        };
        if p.tokens.get(p.pos).is_none_or(|t| t.text != "{") {
            return Err(p.error("UPSERT requires an object body"));
        }
        let value = p.expr(0)?;
        let returning = p.returning()?;
        return Ok(Statement::Upsert {
            table,
            target,
            value,
            returning,
        });
    }
    p.pos = 0;
    if p.eat("REMOVE") && p.eat("FIELD") {
        let path = p.path()?;
        if !p.eat("ON") {
            return Err(p.error("expected ON"));
        }
        let table = p.name()?;
        if !p.end() {
            return Err(p.error("unexpected REMOVE FIELD clause"));
        }
        return Ok(Statement::RemoveField { table, path });
    }
    p.pos = 0;
    if p.eat("INFO") && p.eat("FOR") {
        let scope = p.name()?.to_ascii_lowercase();
        let name = match scope.as_str() {
            "db" => None,
            "table" | "index" => Some(p.name()?),
            _ => return Err(p.error("expected DB, TABLE, or INDEX")),
        };
        if !p.end() {
            return Err(p.error("unexpected INFO clause"));
        }
        return Ok(Statement::Info { scope, name });
    }
    p.pos = 0;
    if p.eat("DEFINE") && p.eat("FIELD") {
        let overwrite = p.eat("OVERWRITE");
        let path = p.path()?;
        if !p.eat("ON") {
            return Err(p.error("expected ON"));
        }
        let table = p.name()?;
        if !p.eat("TYPE") {
            return Err(p.error("expected TYPE"));
        }
        let kind = p.name()?.to_ascii_lowercase();
        let target = if p.eat("<") {
            let target = p.name()?;
            if !p.eat(">") {
                return Err(p.error("expected >"));
            }
            Some(target)
        } else {
            None
        };
        let required = p.eat("REQUIRED");
        let nullable = p.eat("NULLABLE");
        if !p.end() {
            return Err(p.error("unsupported field definition clause"));
        }
        return Ok(Statement::DefineField {
            table,
            path,
            kind,
            target,
            required,
            nullable,
            overwrite,
        });
    }
    p.pos = 0;
    if p.eat("CREATE") {
        let unique = p.eat("UNIQUE");
        if p.eat("INDEX") {
            let parsed = (|| -> Result<Statement> {
                let if_not_exists = p.eat("IF");
                if if_not_exists && !(p.eat("NOT") && p.eat("EXISTS")) {
                    return Err(p.error("expected IF NOT EXISTS"));
                }
                let name = p.name()?;
                if !p.eat("ON") {
                    return Err(p.error("expected ON"));
                }
                let table = p.name()?;
                if !p.eat("(") {
                    return Err(p.error("expected ("));
                }
                let path = p.path()?;
                if !p.eat(")") || !p.end() {
                    return Err(p.error("expected index end"));
                }
                Ok(Statement::CreateIndex {
                    if_not_exists,
                    table,
                    name,
                    path,
                    unique,
                    sql: input.into(),
                })
            })();
            return Ok(parsed.unwrap_or_else(|_| Statement::Sql(input.into())));
        }
    }
    p.pos = 0;
    if p.eat("CREATE") && p.eat("TABLE") {
        let if_not_exists = p.eat("IF");
        if if_not_exists && !(p.eat("NOT") && p.eat("EXISTS")) {
            return Ok(Statement::Sql(input.into()));
        }
        if let Ok(name) = p.name() {
            if p.end() {
                return Ok(Statement::CreateCollection {
                    name,
                    if_not_exists,
                });
            }
        }
    }
    p.pos = 0;
    if p.eat("INSERT") && p.eat("INTO") {
        let table = p.name()?;
        if p.tokens.get(p.pos).is_some_and(|t| t.text == "{") || p.eat("DOCUMENT") {
            let value = p.expr(0)?;
            let returning = p.returning()?;
            return Ok(Statement::Insert {
                table,
                value,
                returning,
            });
        }
    }
    for verb in ["SELECT", "UPDATE", "DELETE"] {
        p.pos = 0;
        if !p.eat(verb) {
            continue;
        }
        if verb == "DELETE" && !p.eat("FROM") {
            continue;
        }
        let start = p.pos;
        if p.tokens.get(start + 1).is_none_or(|t| t.text != ":")
            || p.tokens.get(start + 2).is_some_and(|t| t.text == ":")
        {
            continue;
        }
        let target = p.record()?;
        return match verb {
            "SELECT" => {
                if !p.end() {
                    return Ok(Statement::Sql(input.into()));
                }
                Ok(Statement::SelectRecord(target))
            }
            "UPDATE" => {
                if p.tokens.get(p.pos).is_some_and(|t| {
                    t.kind == Kind::Word
                        && (t.text.eq_ignore_ascii_case("SET")
                            || t.text.eq_ignore_ascii_case("UNSET"))
                }) {
                    return Ok(Statement::Sql(input.into()));
                }
                let value = p.expr(0)?;
                let returning = p.returning()?;
                Ok(Statement::Patch {
                    target,
                    value,
                    returning,
                })
            }
            _ => {
                let returning = p.returning()?;
                Ok(Statement::Delete { target, returning })
            }
        };
    }
    Ok(Statement::Sql(input.into()))
}

#[cfg(test)]
mod tests;
