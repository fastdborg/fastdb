//! Logical collection SELECT lowering into the pinned SQLite AST.
use crate::{quote, Collection, Connection, Error, Parameters, QueryResult, Result, Value};
use turso_parser::{ast::*, parser::Parser};

#[derive(Clone)]
struct Source {
    alias: String,
    collection: Option<Collection>,
}
struct Scope {
    sources: Vec<Source>,
}
impl Scope {
    fn field(&self, expr: &Expr) -> Result<Option<(usize, Vec<String>)>> {
        let parts = match expr {
            Expr::Id(n) | Expr::Name(n) => vec![n.as_str().to_owned()],
            Expr::Qualified(a, b) => vec![a.as_str().into(), b.as_str().into()],
            Expr::DoublyQualified(a, b, c) => {
                vec![a.as_str().into(), b.as_str().into(), c.as_str().into()]
            }
            Expr::FieldAccess { base, field, .. } => {
                if let Some((i, mut path)) = self.field(base)? {
                    path.push(field.as_str().into());
                    return Ok(Some((i, path)));
                }
                return Ok(None);
            }
            _ => return Ok(None),
        };
        if parts.len() == 1 {
            if matches!(expr, Expr::Id(n) if !n.quoted() && (n.as_str().eq_ignore_ascii_case("true") || n.as_str().eq_ignore_ascii_case("false")))
            {
                return Ok(None);
            }
            if self.sources.len() != 1 {
                return Err(Error::Validation(
                    "qualify fields in collection joins".into(),
                ));
            }
            return Ok(self.sources[0].collection.as_ref().map(|_| (0, parts)));
        }
        let Some(i) = self
            .sources
            .iter()
            .position(|s| s.alias.eq_ignore_ascii_case(&parts[0]))
        else {
            return Ok(None);
        };
        Ok(self.sources[i]
            .collection
            .as_ref()
            .map(|_| (i, parts[1..].to_vec())))
    }
    fn accessor(&self, i: usize, path: &[String], typed: bool) -> Result<Expr> {
        if !typed && path == ["id"] {
            return expression(&format!("{}.id", quote(&self.sources[i].alias)));
        }
        let path = serde_json::to_string(path)?.replace('\'', "''");
        expression(&format!(
            "{}({}.doc, '{}')",
            if typed {
                "__fastdb_value"
            } else {
                "__fastdb_scalar"
            },
            quote(&self.sources[i].alias),
            path
        ))
    }
    fn lower(&self, expr: &mut Expr) -> Result<()> {
        if let Some((i, path)) = self.field(expr)? {
            *expr = self.accessor(i, &path, false)?;
            return Ok(());
        }
        match expr {
            Expr::Binary(a, _, b) => {
                self.lower(a)?;
                self.lower(b)?;
            }
            Expr::Unary(_, e)
            | Expr::IsNull(e)
            | Expr::NotNull(e)
            | Expr::Cast { expr: e, .. }
            | Expr::Collate(e, _) => self.lower(e)?,
            Expr::Between {
                lhs, start, end, ..
            } => {
                self.lower(lhs)?;
                self.lower(start)?;
                self.lower(end)?;
            }
            Expr::Like {
                lhs, rhs, escape, ..
            } => {
                self.lower(lhs)?;
                self.lower(rhs)?;
                if let Some(e) = escape {
                    self.lower(e)?;
                }
            }
            Expr::InList { lhs, rhs, .. } => {
                self.lower(lhs)?;
                for e in rhs {
                    self.lower(e)?;
                }
            }
            Expr::Parenthesized(es) => {
                for e in es {
                    self.lower(e)?;
                }
            }
            Expr::Case {
                base,
                when_then_pairs,
                else_expr,
            } => {
                if let Some(e) = base {
                    self.lower(e)?;
                }
                for (a, b) in when_then_pairs {
                    self.lower(a)?;
                    self.lower(b)?;
                }
                if let Some(e) = else_expr {
                    self.lower(e)?;
                }
            }
            Expr::FunctionCall {
                args,
                order_by,
                within_group,
                filter_over,
                ..
            } => {
                if filter_over.over_clause.is_some() {
                    return Err(unsupported("window expressions"));
                }
                for e in args {
                    self.lower(e)?;
                }
                for s in order_by.iter_mut().chain(within_group) {
                    self.lower(&mut s.expr)?;
                }
                if let Some(e) = &mut filter_over.filter_clause {
                    self.lower(e)?;
                }
            }
            Expr::FunctionCallStar { filter_over, .. } => {
                if filter_over.over_clause.is_some() {
                    return Err(unsupported("window expressions"));
                }
                if let Some(e) = &mut filter_over.filter_clause {
                    self.lower(e)?;
                }
            }
            Expr::Literal(_)
            | Expr::Variable(_)
            | Expr::Id(_)
            | Expr::Name(_)
            | Expr::Qualified(..)
            | Expr::DoublyQualified(..) => {}
            _ => return Err(unsupported("this collection expression")),
        }
        Ok(())
    }
}
fn unsupported(feature: &str) -> Error {
    Error::Unsupported(format!("{feature} is not implemented for collections"))
}
pub(crate) fn parsed(sql: &str) -> Result<Cmd> {
    let mut parser = Parser::new(sql.as_bytes());
    let cmd = parser
        .next_cmd()
        .map_err(|e| Error::Validation(e.to_string()))?
        .ok_or_else(|| Error::Validation("empty query".into()))?;
    if parser
        .next_cmd()
        .map_err(|e| Error::Validation(e.to_string()))?
        .is_some()
    {
        return Err(unsupported("multiple statements"));
    }
    Ok(cmd)
}
fn expression(sql: &str) -> Result<Expr> {
    let Cmd::Stmt(Stmt::Select(select)) = parsed(&format!("SELECT {sql}"))? else {
        return Err(unsupported("expression"));
    };
    let OneSelect::Select { mut columns, .. } = select.body.select else {
        return Err(unsupported("expression"));
    };
    let ResultColumn::Expr(e, _) = columns.remove(0) else {
        return Err(unsupported("expression"));
    };
    Ok(*e)
}
fn source(connection: &Connection, table: &SelectTable) -> Result<Source> {
    let SelectTable::Table(name, alias, indexed) = table else {
        return Err(unsupported("subqueries and table functions"));
    };
    if name
        .name
        .as_str()
        .to_ascii_lowercase()
        .starts_with("__fastdb_")
    {
        return Err(unsupported("managed storage access"));
    }
    let collection = match connection.catalog(name.name.as_str()) {
        Ok(c) => Some(c),
        Err(Error::NotFound(_)) => None,
        Err(Error::Validation(_))
            if name
                .name
                .as_str()
                .to_ascii_lowercase()
                .starts_with("sqlite_") =>
        {
            None
        }
        Err(e) => return Err(e),
    };
    if collection.is_some()
        && (name
            .db_name
            .as_ref()
            .is_some_and(|n| !n.as_str().eq_ignore_ascii_case("main"))
            || indexed.is_some())
    {
        return Err(unsupported(
            "attached collections or explicit INDEXED clauses",
        ));
    }
    Ok(Source {
        alias: alias
            .as_ref()
            .map_or(name.name.as_str(), |a| a.name().as_str())
            .into(),
        collection,
    })
}
fn constant(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_) | Expr::Variable(_) => true,
        Expr::Unary(_, e) => constant(e),
        Expr::FunctionCall { name, args, .. } if name.as_str() == "__fastdb_record_value" => {
            args.iter().all(|e| constant(e))
        }
        _ => false,
    }
}
fn indexed_equality(
    scope: &Scope,
    source_index: usize,
    predicate: &Expr,
) -> Result<Option<(crate::Index, Expr)>> {
    let Expr::Binary(lhs, op, rhs) = predicate else {
        return Ok(None);
    };
    if *op == Operator::And {
        return Ok(
            indexed_equality(scope, source_index, lhs)?.or(indexed_equality(
                scope,
                source_index,
                rhs,
            )?),
        );
    }
    if *op != Operator::Equals {
        return Ok(None);
    }
    for (field, key) in [(lhs, rhs), (rhs, lhs)] {
        if !constant(key) {
            continue;
        }
        if let Some((i, path)) = scope.field(field)? {
            if i != source_index {
                continue;
            }
            if let Some(index) = scope.sources[i]
                .collection
                .as_ref()
                .and_then(|c| c.indexes.iter().find(|idx| idx.path == path))
            {
                return Ok(Some((index.clone(), *key.clone())));
            }
        }
    }
    Ok(None)
}
fn lower_source(
    table: &mut SelectTable,
    source: &Source,
    candidate: Option<(crate::Index, Expr)>,
) -> Result<()> {
    let Some(c) = &source.collection else {
        return Ok(());
    };
    if let Some((index, key)) = candidate {
        let Cmd::Stmt(Stmt::Select(mut select)) = parsed(&format!(
            "SELECT c.id, c.doc FROM {} AS i JOIN {} AS c ON c.id = i.id WHERE i.key = NULL",
            quote(&index.storage),
            quote(&c.storage)
        ))?
        else {
            return Err(unsupported("index lowering"));
        };
        let OneSelect::Select {
            where_clause: Some(predicate),
            ..
        } = &mut select.body.select
        else {
            return Err(unsupported("index predicate"));
        };
        let Expr::Binary(_, _, rhs) = predicate.as_mut() else {
            return Err(unsupported("index equality"));
        };
        *rhs = Box::new(key);
        *table = SelectTable::Select(select, Some(As::As(Name::exact(source.alias.clone()))));
    } else {
        *table = SelectTable::Table(
            QualifiedName::single(Name::exact(c.storage.clone())),
            Some(As::As(Name::exact(source.alias.clone()))),
            None,
        );
    }
    Ok(())
}
pub(crate) fn expand_records(sql: &str) -> Result<String> {
    let tokens = fastql_parser::tokenize(sql)?;
    let mut out = String::new();
    let mut copied = 0;
    let mut i = 0;
    while i + 2 < tokens.len() {
        if i + 4 < tokens.len()
            && tokens[i].text.eq_ignore_ascii_case("type")
            && tokens[i].kind == fastql_parser::Kind::Word
            && tokens[i + 1].text == ":"
            && tokens[i + 2].text == ":"
            && tokens[i + 3].text.eq_ignore_ascii_case("record")
            && tokens[i + 4].text == "("
            && tokens[i].end == tokens[i + 1].start
            && tokens[i + 1].end == tokens[i + 2].start
            && tokens[i + 2].end == tokens[i + 3].start
        {
            out.push_str(&sql[copied..tokens[i].start]);
            out.push_str("__fastdb_record_value");
            copied = tokens[i + 3].end;
            i += 4;
            continue;
        }
        let a = &tokens[i];
        let colon = &tokens[i + 1];
        let key = &tokens[i + 2];
        if matches!(
            a.kind,
            fastql_parser::Kind::Word | fastql_parser::Kind::Identifier
        ) && colon.text == ":"
            && a.end == colon.start
            && colon.end == key.start
            && key.text != ":"
        {
            let end = if matches!(key.text.as_str(), "-" | "+") {
                tokens.get(i + 3).map_or(key.end, |t| t.end)
            } else {
                key.end
            };
            if let fastql_parser::Statement::SelectRecord(record) =
                fastql_parser::parse(&format!("SELECT {}", &sql[a.start..end]))?
            {
                out.push_str(&sql[copied..a.start]);
                let key = match record.key {
                    crate::Key::Integer(i) => i.to_string(),
                    crate::Key::String(s) => format!("'{}'", s.replace('\'', "''")),
                };
                out.push_str(&format!(
                    "__fastdb_record_value('{}', {})",
                    record.table.replace('\'', "''"),
                    key
                ));
                copied = end;
                while i < tokens.len() && tokens[i].end <= end {
                    i += 1;
                }
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&sql[copied..]);
    Ok(out)
}
impl Connection {
    pub(crate) fn collection_select(
        &self,
        sql: &str,
        params: &Parameters,
    ) -> Result<Option<QueryResult>> {
        // Only route statements that reference a collection table. Pure SQL
        // retains the baseline preparation/error path and original spelling.
        self.collection_select_internal(sql, params, false)
    }
    pub(crate) fn collection_select_internal(
        &self,
        sql: &str,
        params: &Parameters,
        trusted: bool,
    ) -> Result<Option<QueryResult>> {
        self.collection_select_options(sql, params, trusted, trusted)
    }
    pub(crate) fn collection_select_subset(
        &self,
        sql: &str,
        params: &Parameters,
    ) -> Result<Option<QueryResult>> {
        self.collection_select_options(sql, params, false, true)
    }
    fn collection_select_options(
        &self,
        sql: &str,
        params: &Parameters,
        trusted: bool,
        ignore_unused: bool,
    ) -> Result<Option<QueryResult>> {
        let expanded = expand_records(sql)?;
        let Ok(mut cmd) = parsed(&expanded) else {
            return Ok(None);
        };
        let explain = matches!(cmd, Cmd::ExplainQueryPlan(_) | Cmd::Explain(_));
        let select = match &mut cmd {
            Cmd::Stmt(Stmt::Select(s))
            | Cmd::Explain(Stmt::Select(s))
            | Cmd::ExplainQueryPlan(Stmt::Select(s)) => s,
            _ => return Ok(None),
        };
        let OneSelect::Select {
            columns,
            from,
            where_clause,
            group_by,
            window_clause,
            distinctness,
            ..
        } = &mut select.body.select
        else {
            return Ok(None);
        };
        let mut sources = Vec::new();
        if let Some(from) = from {
            let first = match source(self, &from.select) {
                Ok(s) => s,
                Err(Error::Unsupported(_)) => return Ok(None),
                Err(e) => return Err(e),
            };
            sources.push(first);
            for join in &from.joins {
                match source(self, &join.table) {
                    Ok(s) => sources.push(s),
                    Err(Error::Unsupported(_)) => return Ok(None),
                    Err(e) => return Err(e),
                }
            }
        }
        if sources.iter().all(|s| s.collection.is_none()) && expanded == sql {
            return Ok(None);
        }
        if matches!(distinctness, Some(Distinctness::Distinct)) {
            return Err(unsupported("DISTINCT over typed projections"));
        }
        if select.with.is_some()
            || !select.body.compounds.is_empty()
            || group_by.is_some()
            || !window_clause.is_empty()
        {
            return Err(unsupported("CTEs, compound SELECT, grouping or windows"));
        }
        let scope = Scope { sources };
        for (i, s) in scope.sources.iter().enumerate() {
            if scope.sources[..i]
                .iter()
                .any(|other| other.alias.eq_ignore_ascii_case(&s.alias))
            {
                return Err(Error::Validation("duplicate table alias".into()));
            }
        }
        // Validate user expressions before introducing any internal function or
        // storage name. The existing write guard continues covering other SQL.
        for token in fastql_parser::tokenize(sql)? {
            if trusted {
                break;
            }
            if matches!(
                token.kind,
                fastql_parser::Kind::Word | fastql_parser::Kind::Identifier
            ) && token.text.to_ascii_lowercase().starts_with("__fastdb_")
            {
                return Err(unsupported("managed names"));
            }
        }
        let mut typed = Vec::new();
        let mut names = Vec::new();
        let mut rewritten = Vec::new();
        for column in columns.iter() {
            match column {
                ResultColumn::Star | ResultColumn::TableStar(_) => {
                    let source_index = match column {
                        ResultColumn::Star if scope.sources.len() == 1 => 0,
                        ResultColumn::TableStar(name) => scope
                            .sources
                            .iter()
                            .position(|s| s.alias.eq_ignore_ascii_case(name.as_str()))
                            .ok_or_else(|| Error::Validation("unknown star qualifier".into()))?,
                        _ => return Err(unsupported("unqualified star in collection joins")),
                    };
                    if scope.sources[source_index].collection.is_none() {
                        return Err(unsupported("relational star in mixed queries"));
                    }
                    rewritten.push(ResultColumn::Expr(
                        Box::new(scope.accessor(source_index, &[], true)?),
                        Some(As::As(Name::exact("document".into()))),
                    ));
                    typed.push(true);
                    names.push("document".to_owned());
                }
                ResultColumn::Expr(expr, alias) => {
                    let mut expr = *expr.clone();
                    let field = scope.field(&expr)?;
                    let name = alias
                        .as_ref()
                        .filter(|a| a.is_explicit())
                        .map(|a| a.name().as_str().to_owned())
                        .unwrap_or_else(|| {
                            field.as_ref().map_or_else(
                                || {
                                    expr.to_string()
                                        .replace("__fastdb_record_value", "type::record")
                                },
                                |(_, path)| path.last().expect("nonempty path").clone(),
                            )
                        });
                    if let Some((i, path)) = field {
                        expr = scope.accessor(i, &path, true)?;
                        typed.push(true);
                    } else {
                        scope.lower(&mut expr)?;
                        typed.push(matches!(&expr, Expr::FunctionCall{name,..} if name.as_str()=="__fastdb_record_value"));
                    }
                    names.push(name.clone());
                    rewritten.push(ResultColumn::Expr(
                        Box::new(expr),
                        Some(As::As(Name::exact(name))),
                    ));
                }
            }
        }
        for (i, name) in names.iter().enumerate() {
            if names[..i].contains(name) {
                return Err(Error::Validation(
                    "duplicate projection names; use AS".into(),
                ));
            }
        }
        *columns = rewritten;
        let candidates = scope
            .sources
            .iter()
            .enumerate()
            .map(|(i, _)| {
                where_clause
                    .as_ref()
                    .map_or(Ok(None), |p| indexed_equality(&scope, i, p))
            })
            .collect::<Result<Vec<_>>>()?;
        if let Some(from) = from {
            lower_source(&mut from.select, &scope.sources[0], candidates[0].clone())?;
            for (i, join) in from.joins.iter_mut().enumerate() {
                // An outer join WHERE predicate must remain outside the join; using
                // a filtered source would change NULL-extension behavior.
                lower_source(&mut join.table, &scope.sources[i + 1], None)?;
                if let Some(constraint) = &mut join.constraint {
                    match constraint {
                        JoinConstraint::On(e) => scope.lower(e)?,
                        JoinConstraint::Using(_) => return Err(unsupported("USING joins")),
                    }
                }
                if matches!(join.operator,JoinOperator::TypedJoin(Some(t)) if t.contains(JoinType::NATURAL))
                {
                    return Err(unsupported("NATURAL joins"));
                }
            }
        }
        if let Some(expr) = where_clause {
            scope.lower(expr)?;
        }
        for sorted in &mut select.order_by {
            // Aliases refer to the original expression, not the encoded typed
            // projection, so sorting keeps SQL scalar semantics.
            let alias_index = match sorted.expr.as_ref() {
                Expr::Id(n) | Expr::Name(n) => names
                    .iter()
                    .position(|name| name.eq_ignore_ascii_case(n.as_str())),
                Expr::Literal(Literal::Numeric(n)) => n
                    .parse::<usize>()
                    .ok()
                    .filter(|i| *i > 0 && *i <= columns.len())
                    .map(|i| i - 1),
                _ => None,
            };
            if let Some(i) = alias_index {
                if typed[i] {
                    let ResultColumn::Expr(e, _) = &columns[i] else {
                        unreachable!("rewritten projections");
                    };
                    let mut e = *e.clone();
                    if let Expr::FunctionCall { name, .. } = &mut e {
                        if name.as_str() == "__fastdb_value" {
                            *name = Name::exact("__fastdb_sort".into());
                        } else {
                            e = expression(&format!("__fastdb_sort_encoded({e})"))?;
                        }
                    }
                    sorted.expr = Box::new(e);
                }
            } else if let Some((i, path)) = scope.field(&sorted.expr)? {
                let mut e = scope.accessor(i, &path, true)?;
                if let Expr::FunctionCall { name, .. } = &mut e {
                    *name = Name::exact("__fastdb_sort".into());
                }
                sorted.expr = Box::new(e);
            } else {
                scope.lower(&mut sorted.expr)?;
            }
        }
        let lowered = cmd.to_string();
        let mut statement = self.engine.prepare(&lowered)?;
        for (name, value) in params {
            let Some(index) = crate::bind_index(&statement, name) else {
                if ignore_unused {
                    continue;
                }
                return Err(Error::Parameter(name.clone()));
            };
            statement.bind_at(index, crate::index_scalar(value)?)?;
        }
        let engine_names = (0..statement.num_columns())
            .map(|i| statement.get_column_name(i).into_owned())
            .collect();
        let mut rows = Vec::new();
        for row in statement.run_collect_rows()? {
            let mut output = Vec::new();
            for (i, value) in row.into_iter().enumerate() {
                if !explain && typed[i] {
                    output.push(match value {
                        turso_core::Value::Blob(b) => Value::decode(&b)?,
                        turso_core::Value::Null => Value::Null,
                        _ => return Err(Error::Storage("invalid typed projection".into())),
                    });
                } else {
                    output.push(crate::from_engine(value));
                }
            }
            rows.push(output);
        }
        Ok(Some(QueryResult {
            columns: if explain { engine_names } else { names },
            rows,
            affected: 0,
        }))
    }
}
