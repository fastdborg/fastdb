//! Logical collection SELECT lowering into the pinned SQLite AST.
use crate::{quote, Collection, Connection, Error, Parameters, QueryResult, Result, Value};
use turso_parser::{ast::*, parser::Parser};

#[derive(Clone)]
struct Source {
    table: SelectTable,
    alias: String,
    collection: Option<Collection>,
}
struct Scope {
    sources: Vec<Source>,
    params: Parameters,
    consumed: std::cell::RefCell<std::collections::BTreeSet<String>>,
    fetched_aliases: std::cell::RefCell<std::collections::BTreeSet<String>>,
    standalone_aliases: std::cell::RefCell<std::collections::BTreeMap<String, (Expr, bool)>>,
}
impl Scope {
    fn field(&self, expr: &Expr) -> Result<Option<(usize, Vec<String>)>> {
        let parts = match expr {
            Expr::Id(n) | Expr::Name(n) => vec![n.as_str().to_owned()],
            Expr::Qualified(a, b) => vec![a.as_str().into(), b.as_str().into()],
            Expr::DoublyQualified(a, b, c) => {
                vec![a.as_str().into(), b.as_str().into(), c.as_str().into()]
            }
            Expr::FunctionCall { name, args, .. }
                if name.as_str() == "__fastdb_path" && args.len() >= 4 =>
            {
                args.iter()
                    .map(|arg| match arg.as_ref() {
                        Expr::Id(name) | Expr::Name(name) => Ok(name.as_str().to_owned()),
                        _ => Err(unsupported("invalid nested field path")),
                    })
                    .collect::<Result<Vec<_>>>()?
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
            if self
                .fetched_aliases
                .borrow()
                .contains(&parts[0].to_ascii_lowercase())
            {
                return Err(unsupported(
                    "filtering on fetched aliases; qualify the stored source field if intended",
                ));
            }
            if matches!(expr, Expr::Id(n) if !n.quoted() && (n.as_str().eq_ignore_ascii_case("true") || n.as_str().eq_ignore_ascii_case("false")))
            {
                return Ok(None);
            }
            if self.sources.is_empty() {
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
    fn standalone_alias(&self, expr: &Expr) -> Option<(Expr, bool)> {
        let name = match expr {
            Expr::Id(n)
                if !n.quoted()
                    && (n.as_str().eq_ignore_ascii_case("true")
                        || n.as_str().eq_ignore_ascii_case("false")) =>
            {
                return None
            }
            Expr::Id(n) | Expr::Name(n) => n.as_str(),
            _ => return None,
        };
        self.standalone_aliases
            .borrow()
            .get(&name.to_ascii_lowercase())
            .cloned()
    }
    fn preserved(&self, expr: &mut Expr) -> Result<bool> {
        if let Some((value, typed)) = self.standalone_alias(expr) {
            if typed {
                *expr = value;
            }
            return Ok(typed);
        }
        if let Some((i, path)) = self.field(expr)? {
            *expr = self.accessor(i, &path, true)?;
            return Ok(true);
        }
        if let Expr::Variable(var) = expr {
            let name = var
                .name
                .as_ref()
                .map_or_else(|| format!("?{}", var.index), |s| s.to_string());
            let value = self
                .params
                .get(&name)
                .ok_or_else(|| Error::Parameter(name.clone()))?;
            let bytes = value.encode()?;
            let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
            self.consumed.borrow_mut().insert(name);
            *expr = expression(&format!("X'{hex}'"))?;
            return Ok(true);
        }
        if self.helper(expr)? {
            return Ok(true);
        }
        if matches!(expr, Expr::FunctionCall{name,..} if name.as_str()=="__fastdb_record_value") {
            self.lower(expr)?;
            return Ok(true);
        }
        Ok(false)
    }
    fn typed(&self, expr: &mut Expr) -> Result<()> {
        if !self.preserved(expr)? {
            self.lower(expr)?;
            *expr = expression(&format!("__fastdb_pack({expr})"))?;
        }
        Ok(())
    }
    fn helper(&self, expr: &mut Expr) -> Result<bool> {
        if let Expr::Case {
            base,
            when_then_pairs,
            else_expr,
        } = expr
        {
            if let Some(base) = base {
                self.lower(base)?;
            }
            for (condition, value) in when_then_pairs {
                self.lower(condition)?;
                self.typed(value)?;
            }
            if let Some(value) = else_expr {
                self.typed(value)?;
            }
            return Ok(true);
        }
        if let Expr::Parenthesized(es) = expr {
            if let [e] = es.as_mut_slice() {
                return self.preserved(e);
            }
            return Ok(false);
        }
        let Expr::FunctionCall {
            name,
            args,
            filter_over,
            order_by,
            within_group,
            distinctness,
        } = expr
        else {
            return Ok(false);
        };
        if matches!(
            name.as_str().to_ascii_lowercase().as_str(),
            "vector32"
                | "vector64"
                | "vector32_sparse"
                | "vector8"
                | "vector1bit"
                | "vector_concat"
                | "vector_slice"
        ) {
            let expected = match name.as_str().to_ascii_lowercase().as_str() {
                "vector_concat" => 2,
                "vector_slice" => 3,
                _ => 1,
            };
            if args.len() != expected
                || distinctness.is_some()
                || filter_over.over_clause.is_some()
                || filter_over.filter_clause.is_some()
                || !order_by.is_empty()
                || !within_group.is_empty()
            {
                return Err(unsupported("vector constructor arguments"));
            }
            let vector_args = if name.as_str().eq_ignore_ascii_case("vector_concat") {
                2
            } else {
                1
            };
            for (i, arg) in args.iter_mut().enumerate() {
                if i < vector_args {
                    self.typed(arg)?;
                    *arg = Box::new(expression(&format!("__fastdb_vector_input({arg})"))?);
                } else {
                    self.lower(arg)?;
                }
            }
            if name.as_str().eq_ignore_ascii_case("vector_concat") {
                *name = Name::exact("__fastdb_vector_concat".into());
            }
            *expr = expression(&format!("__fastdb_vector_value({expr})"))?;
            return Ok(true);
        }
        let is_null_helper = name.as_str().eq_ignore_ascii_case("coalesce")
            || name.as_str().eq_ignore_ascii_case("ifnull");
        if (is_null_helper || name.as_str().starts_with("__fastdb_h_"))
            && (filter_over.filter_clause.is_some()
                || filter_over.over_clause.is_some()
                || !order_by.is_empty()
                || !within_group.is_empty()
                || distinctness.is_some())
        {
            return Err(unsupported("aggregate modifiers on document helpers"));
        }
        if name.as_str().eq_ignore_ascii_case("coalesce")
            || name.as_str().eq_ignore_ascii_case("ifnull")
        {
            if args.len() < 2 || (name.as_str().eq_ignore_ascii_case("ifnull") && args.len() != 2) {
                return Err(Error::Validation("invalid null-helper arity".into()));
            }
            for arg in args.iter_mut() {
                self.typed(arg)?;
                *arg = Box::new(expression(&format!("__fastdb_nullable({arg})"))?);
            }
            return Ok(true);
        }
        let Some(helper) = name.as_str().strip_prefix("__fastdb_h_") else {
            return Ok(false);
        };
        let helper = helper.to_owned();
        if filter_over.filter_clause.is_some()
            || filter_over.over_clause.is_some()
            || !order_by.is_empty()
            || !within_group.is_empty()
        {
            return Err(unsupported("aggregate modifiers on document helpers"));
        }
        if helper == "doc_row" {
            let [arg] = args.as_slice() else {
                return Err(Error::Validation(
                    "doc::row expects collection alias".into(),
                ));
            };
            let alias = match arg.as_ref() {
                Expr::Id(n) | Expr::Name(n) => n.as_str(),
                _ => {
                    return Err(Error::Validation(
                        "doc::row expects collection alias".into(),
                    ))
                }
            };
            let i = self
                .sources
                .iter()
                .position(|s| s.collection.is_some() && s.alias.eq_ignore_ascii_case(alias))
                .ok_or_else(|| Error::Validation("unknown collection alias".into()))?;
            *expr = self.accessor(i, &[], true)?;
            return Ok(true);
        }
        for arg in args.iter_mut() {
            self.typed(arg)?;
        }
        let tail = args
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        *expr = expression(&format!(
            "__fastdb_helper('{helper}'{})",
            if tail.is_empty() {
                String::new()
            } else {
                format!(",{tail}")
            }
        ))?;
        Ok(true)
    }
    fn lower_window(&self, window: &mut Window) -> Result<()> {
        for expr in &mut window.partition_by {
            self.lower(expr)?;
        }
        for sorted in &mut window.order_by {
            self.lower(&mut sorted.expr)?;
        }
        if let Some(frame) = &mut window.frame_clause {
            for bound in std::iter::once(&mut frame.start).chain(frame.end.iter_mut()) {
                if let FrameBound::Preceding(expr) | FrameBound::Following(expr) = bound {
                    self.lower(expr)?;
                }
            }
        }
        Ok(())
    }
    fn lower(&self, expr: &mut Expr) -> Result<()> {
        if let Some((value, typed)) = self.standalone_alias(expr) {
            *expr = if typed {
                expression(&format!("__fastdb_unwrap({value})"))?
            } else {
                value
            };
            return Ok(());
        }
        if let Expr::FunctionCall { name, args, .. } = expr {
            if matches!(
                name.as_str().to_ascii_lowercase().as_str(),
                "vector_distance_cos"
                    | "vector_distance_l2"
                    | "vector_distance_jaccard"
                    | "vector_distance_dot"
                    | "vector_extract"
            ) {
                for arg in args {
                    self.typed(arg)?;
                    *arg = Box::new(expression(&format!("__fastdb_vector_input({arg})"))?);
                }
                return Ok(());
            }
        }
        if matches!(expr, Expr::FunctionCall {name,..} if name.as_str()=="__fastdb_fetch") {
            return Err(unsupported(
                "record::fetch is allowed only as a top-level SELECT projection",
            ));
        }
        if self.helper(expr)? {
            *expr = expression(&format!("__fastdb_unwrap({expr})"))?;
            return Ok(());
        }
        if let Some((i, path)) = self.field(expr)? {
            *expr = self.accessor(i, &path, false)?;
            return Ok(());
        }
        match expr {
            Expr::Binary(a, op, b) => {
                if matches!(
                    op,
                    Operator::Less
                        | Operator::LessEquals
                        | Operator::Greater
                        | Operator::GreaterEquals
                ) {
                    let (mut left, mut right) = (*a.clone(), *b.clone());
                    if self.preserved(&mut left)? && self.preserved(&mut right)? {
                        *expr = expression(&format!("__fastdb_compare({left}, {right}) {op} 0"))?;
                        return Ok(());
                    }
                }
                self.lower(a)?;
                self.lower(b)?;
            }
            Expr::Unary(_, e)
            | Expr::IsNull(e)
            | Expr::NotNull(e)
            | Expr::Cast { expr: e, .. }
            | Expr::Collate(e, _) => self.lower(e)?,
            Expr::Between {
                lhs,
                start,
                end,
                not,
            } => {
                let (mut value, mut lower, mut upper) =
                    (*lhs.clone(), *start.clone(), *end.clone());
                if self.preserved(&mut value)?
                    && self.preserved(&mut lower)?
                    && self.preserved(&mut upper)?
                {
                    let negate = if *not { "NOT " } else { "" };
                    *expr = expression(&format!(
                        "{negate}__fastdb_between({value}, {lower}, {upper})"
                    ))?;
                    return Ok(());
                }
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
                if let Some(Over::Window(window)) = &mut filter_over.over_clause {
                    self.lower_window(window)?;
                }
                if filter_over.over_clause.is_some() && !order_by.is_empty() {
                    return Err(unsupported(
                        "aggregate-local ORDER BY with OVER in the pinned engine",
                    ));
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
                if let Some(Over::Window(window)) = &mut filter_over.over_clause {
                    self.lower_window(window)?;
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
fn public_expression_name(expr: &Expr) -> Result<String> {
    use fastql_parser::Kind;
    let sql = expr.to_string();
    let tokens = fastql_parser::tokenize(&sql)?;
    let mut out = String::new();
    let mut copied = 0;
    let mut i = 0;
    while i + 1 < tokens.len() {
        let token = &tokens[i];
        if !matches!(token.kind, Kind::Word | Kind::Identifier) || tokens[i + 1].text != "(" {
            i += 1;
            continue;
        }
        if token.text == "__fastdb_path" {
            let mut end = i + 2;
            let mut parts = Vec::new();
            while end < tokens.len() && matches!(tokens[end].kind, Kind::Word | Kind::Identifier) {
                parts.push(quote(&tokens[end].text));
                end += 1;
                if tokens.get(end).is_some_and(|t| t.text == ",") {
                    end += 1;
                } else {
                    break;
                }
            }
            if parts.len() >= 4 && tokens.get(end).is_some_and(|t| t.text == ")") {
                out.push_str(&sql[copied..token.start]);
                out.push_str(&parts.join("."));
                copied = tokens[end].end;
                i = end + 1;
                continue;
            }
        }
        let public = match token.text.as_str() {
            "__fastdb_record_value" => Some("type::record"),
            "__fastdb_fetch" => Some("record::fetch"),
            "__fastdb_h_string_slugify" => Some("string::slugify"),
            "__fastdb_h_string_normalize" => Some("string::normalize"),
            "__fastdb_h_record_id" => Some("record::id"),
            "__fastdb_h_record_table" => Some("record::table"),
            "__fastdb_h_array_new" => Some("array::new"),
            "__fastdb_h_array_append" => Some("array::append"),
            "__fastdb_h_doc_get" => Some("doc::get"),
            "__fastdb_h_doc_has" => Some("doc::has"),
            "__fastdb_h_doc_row" => Some("doc::row"),
            _ => None,
        };
        if let Some(public) = public {
            out.push_str(&sql[copied..token.start]);
            out.push_str(public);
            copied = token.end;
        }
        i += 1;
    }
    out.push_str(&sql[copied..]);
    Ok(out)
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
        table: table.clone(),
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
fn order_base(expr: &Expr) -> &Expr {
    match expr {
        Expr::Collate(expr, _) => order_base(expr),
        Expr::Parenthesized(exprs) if exprs.len() == 1 => order_base(&exprs[0]),
        _ => expr,
    }
}
fn replace_order_base(expr: &mut Expr, value: Expr) {
    match expr {
        Expr::Collate(expr, _) => replace_order_base(expr, value),
        Expr::Parenthesized(exprs) if exprs.len() == 1 => replace_order_base(&mut exprs[0], value),
        _ => *expr = value,
    }
}

// Match the pinned engine's numeric ordinal recognition. Only one sign
// directly on a literal is recognized; compound constant expressions are not
// evaluated as positions. Zero represents both zero and invalid negative
// positions. usize parsing also matches the engine's range on this platform.
fn projection_position(expr: &Expr) -> Option<usize> {
    match order_base(expr) {
        Expr::Literal(Literal::Numeric(n)) => n.parse().ok(),
        Expr::Unary(UnaryOperator::Positive, inner) => match inner.as_ref() {
            Expr::Literal(Literal::Numeric(n)) => n.parse().ok(),
            _ => None,
        },
        Expr::Unary(UnaryOperator::Negative, inner) => match inner.as_ref() {
            Expr::Literal(Literal::Numeric(n)) if n.parse::<usize>().is_ok() => Some(0),
            _ => None,
        },
        _ => None,
    }
}

fn is_order_output(expr: &Expr, count: usize) -> bool {
    matches!(expr, Expr::Id(name) | Expr::Name(name) if name.as_str().strip_prefix("__fastdb_out_").and_then(|i| i.parse::<usize>().ok()).is_some_and(|i| i < count))
}
fn references_order_output(expr: &Expr, count: usize) -> turso_core::Result<bool> {
    let mut copy = expr.clone();
    let mut found = false;
    turso_core::walk_expr_mut(&mut copy, &mut |expr| {
        found |= is_order_output(expr, count);
        Ok(turso_core::WalkControl::Continue)
    })?;
    Ok(found)
}
// Leave output-dependent arithmetic/functions outside DISTINCT grouping. Lift
// independent source expressions into the inner query so mixed source/alias
// expressions can still access their inputs without reevaluating projections.
fn lift_order_inputs(expr: &mut Expr, columns: &mut Vec<ResultColumn>, count: usize) -> Result<()> {
    turso_core::walk_expr_mut(expr, &mut |expr| {
        if is_order_output(expr, count) || matches!(expr, Expr::Literal(_) | Expr::Variable(_)) {
            return Ok(turso_core::WalkControl::SkipChildren);
        }
        if !references_order_output(expr, count)? {
            let name = format!("__fastdb_order_input_{}", columns.len());
            columns.push(ResultColumn::Expr(
                Box::new(expr.clone()),
                Some(As::As(Name::exact(name.clone()))),
            ));
            *expr = Expr::Id(Name::exact(name));
            return Ok(turso_core::WalkControl::SkipChildren);
        }
        Ok(turso_core::WalkControl::Continue)
    })?;
    Ok(())
}

// Group comparison values in an outer query so aggregate/window evaluation
// happens first and pagination happens after duplicate elimination. Keep each
// original typed projection as the representative output for its group.
fn lower_distinct(
    select: &mut Select,
    typed: &[bool],
    order_outputs: &[Option<usize>],
) -> Result<()> {
    let limit = select.limit.take();
    let mut order = std::mem::take(&mut select.order_by);
    let OneSelect::Select { columns, .. } = &mut select.body.select else {
        return Err(unsupported("DISTINCT source"));
    };
    let mut output = Vec::new();
    let mut keys = Vec::new();
    for (i, column) in columns.iter_mut().enumerate() {
        let ResultColumn::Expr(_, alias) = column else {
            unreachable!("expanded projections")
        };
        let name = format!("__fastdb_out_{i}");
        *alias = Some(As::As(Name::exact(name.clone())));
        output.push(quote(&name));
        keys.push(if typed[i] {
            format!("__fastdb_unwrap({})", quote(&name))
        } else {
            quote(&name)
        });
    }
    for (i, sorted) in order.iter_mut().enumerate() {
        if let Some(output) = order_outputs[i] {
            let name = quote(&format!("__fastdb_out_{output}"));
            replace_order_base(
                &mut sorted.expr,
                expression(&if typed[output] {
                    format!("__fastdb_sort_encoded({name})")
                } else {
                    name
                })?,
            );
            continue;
        }
        lift_order_inputs(&mut sorted.expr, columns, typed.len())?;
        crate::write::validate_returning(&[ResultColumn::Expr(sorted.expr.clone(), None)])
            .map_err(|_| unsupported("aggregate/window expressions over ordering aliases"))?;
    }
    // Keep volatile projections inside their own result-producing query.
    select.limit = Some(Limit {
        expr: Box::new(expression("-1")?),
        offset: None,
    });
    let mut sql = format!(
        "SELECT {} FROM ({select}) GROUP BY {}",
        output.join(","),
        keys.join(",")
    );
    if !order.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(
            &order
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if let Some(limit) = limit {
        sql.push_str(&format!(" LIMIT {}", limit.expr));
        if let Some(offset) = limit.offset {
            sql.push_str(&format!(" OFFSET {offset}"));
        }
    }
    let Cmd::Stmt(Stmt::Select(lowered)) = parsed(&sql)? else {
        unreachable!("generated SELECT")
    };
    *select = lowered;
    Ok(())
}

// The pinned parser only builds names with up to three segments. Encode
// longer paths as a temporary AST expression; Scope resolves it before SQL
// preparation. This marker is never a registered engine function.
pub(crate) fn expand_paths(sql: &str) -> Result<String> {
    use fastql_parser::Kind;
    let tokens = fastql_parser::tokenize(sql)?;
    let is_name = |i: usize| matches!(tokens[i].kind, Kind::Word | Kind::Identifier);
    let mut out = String::new();
    let mut copied = 0;
    let mut i = 0;
    while i < tokens.len() {
        if !is_name(i) {
            i += 1;
            continue;
        }
        let start = i;
        while i + 2 < tokens.len() && tokens[i + 1].text == "." && is_name(i + 2) {
            i += 2;
        }
        if i - start >= 6 {
            if (i - start) / 2 > 64 {
                return Err(Error::Limit("document path nesting exceeds 64".into()));
            }
            out.push_str(&sql[copied..tokens[start].start]);
            out.push_str("__fastdb_path(");
            for part in (start..=i).step_by(2) {
                if part != start {
                    out.push(',');
                }
                out.push_str(&quote(&tokens[part].text));
            }
            out.push(')');
            copied = tokens[i].end;
        }
        i += 1;
    }
    out.push_str(&sql[copied..]);
    Ok(out)
}

pub(crate) fn expand_records(sql: &str) -> Result<String> {
    let tokens = fastql_parser::tokenize(sql)?;
    let mut out = String::new();
    let mut copied = 0;
    let mut i = 0;
    while i + 2 < tokens.len() {
        if i + 4 < tokens.len()
            && tokens[i].kind == fastql_parser::Kind::Word
            && tokens[i + 1].text == ":"
            && tokens[i + 2].text == ":"
            && tokens[i + 3].kind == fastql_parser::Kind::Word
            && tokens[i + 4].text == "("
            && tokens[i].end == tokens[i + 1].start
            && tokens[i + 1].end == tokens[i + 2].start
            && tokens[i + 2].end == tokens[i + 3].start
        {
            let namespace = format!(
                "{}::{}",
                tokens[i].text.to_ascii_lowercase(),
                tokens[i + 3].text.to_ascii_lowercase()
            );
            let mapped = match namespace.as_str() {
                "type::record" => "__fastdb_record_value",
                "string::slugify" => "__fastdb_h_string_slugify",
                "string::normalize" => "__fastdb_h_string_normalize",
                "record::id" => "__fastdb_h_record_id",
                "record::fetch" => "__fastdb_fetch",
                "record::table" => "__fastdb_h_record_table",
                "array::new" => "__fastdb_h_array_new",
                "array::append" => "__fastdb_h_array_append",
                "doc::get" => "__fastdb_h_doc_get",
                "doc::has" => "__fastdb_h_doc_has",
                "doc::row" => "__fastdb_h_doc_row",
                _ => return Err(unsupported("unknown function namespace")),
            };
            out.push_str(&sql[copied..tokens[i].start]);
            out.push_str(mapped);
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
#[derive(Default)]
struct SelectOptions<'a> {
    trusted: bool,
    ignore_unused: bool,
    positional: bool,
    snapshot: Option<Option<&'a crate::Document>>,
    guarded: bool,
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
        self.collection_select_options(
            sql,
            params,
            SelectOptions {
                trusted,
                ignore_unused: trusted,
                ..Default::default()
            },
        )
    }
    pub(crate) fn collection_select_subset(
        &self,
        sql: &str,
        params: &Parameters,
    ) -> Result<Option<QueryResult>> {
        self.collection_select_options(
            sql,
            params,
            SelectOptions {
                ignore_unused: true,
                ..Default::default()
            },
        )
    }
    pub(crate) fn insert_select(&self, sql: &str, params: &Parameters) -> Result<QueryResult> {
        self.collection_select_options(
            sql,
            params,
            SelectOptions {
                trusted: true,
                positional: true,
                ..Default::default()
            },
        )?
        .ok_or_else(|| unsupported("this INSERT SELECT source"))
    }
    pub(crate) fn returning_rows(
        &self,
        table: &QualifiedName,
        columns: &[ResultColumn],
        documents: Vec<crate::Document>,
        params: &Parameters,
    ) -> Result<QueryResult> {
        let affected = documents.len() as i64;
        if columns.is_empty() {
            return Ok(QueryResult::command(affected));
        }
        if matches!(columns, [ResultColumn::Star]) {
            return Ok(QueryResult::documents(documents, affected));
        }
        let projections = columns
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let alias = table
            .alias
            .as_ref()
            .map_or(table.name.as_str(), |n| n.as_str());
        let sql = format!(
            "SELECT {projections} FROM {} AS {}",
            quote(table.name.as_str()),
            quote(alias)
        );
        let mut result = None;
        let inputs = if documents.is_empty() {
            vec![None]
        } else {
            documents.iter().map(Some).collect()
        };
        for snapshot in inputs {
            let row = self
                .collection_select_options(
                    &sql,
                    params,
                    SelectOptions {
                        trusted: true,
                        ignore_unused: true,
                        snapshot: Some(snapshot),
                        ..Default::default()
                    },
                )?
                .ok_or_else(|| unsupported("RETURNING projection"))?;
            let output = result.get_or_insert_with(|| QueryResult {
                columns: row.columns.clone(),
                rows: Vec::new(),
                affected,
            });
            output.rows.extend(row.rows);
        }
        Ok(result.expect("at least metadata projection"))
    }
    fn collection_select_options(
        &self,
        sql: &str,
        params: &Parameters,
        options: SelectOptions<'_>,
    ) -> Result<Option<QueryResult>> {
        let expanded = expand_paths(&expand_records(sql)?)?;
        if !options.guarded
            && fastql_parser::tokenize(&expanded)?
                .iter()
                .any(|t| t.kind == fastql_parser::Kind::Word && t.text == "__fastdb_fetch")
        {
            return self.atomic(|| {
                self.collection_select_options(
                    sql,
                    params,
                    SelectOptions {
                        guarded: true,
                        ..options
                    },
                )
            });
        }
        let SelectOptions {
            trusted,
            ignore_unused,
            positional,
            snapshot,
            guarded: _,
        } = options;
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
        let standalone_typed_parameters = from.is_none()
            && select.with.is_none()
            && select.body.compounds.is_empty()
            && params.values().any(|v| {
                matches!(
                    v,
                    Value::Boolean(_)
                        | Value::Record(_)
                        | Value::Object(_)
                        | Value::Array(_)
                        | Value::Vector(_)
                )
            });
        if !trusted
            && sources.iter().all(|s| s.collection.is_none())
            && expanded == sql
            && !standalone_typed_parameters
        {
            return Ok(None);
        }
        let distinct = !sources.is_empty() && matches!(distinctness, Some(Distinctness::Distinct));
        if distinct {
            *distinctness = None;
        }
        if select.with.is_some() || !select.body.compounds.is_empty() {
            return Err(unsupported("CTEs or compound SELECT"));
        }
        let scope = Scope {
            sources,
            params: params.clone(),
            consumed: Default::default(),
            fetched_aliases: Default::default(),
            standalone_aliases: Default::default(),
        };
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
        *columns = expand_stars(self, &scope, columns)?;
        let original_columns = columns.clone();
        let mut typed = Vec::new();
        let mut fetched = Vec::new();
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
                    fetched.push(false);
                    names.push("document".to_owned());
                }
                ResultColumn::Expr(expr, alias) => {
                    let mut expr = *expr.clone();
                    let field = scope.field(&expr)?;
                    let name = if let Some(alias) = alias.as_ref().filter(|a| a.is_explicit()) {
                        alias.name().as_str().to_owned()
                    } else if let Some((_, path)) = &field {
                        path.last().expect("nonempty path").clone()
                    } else {
                        public_expression_name(&expr)?
                    };
                    let is_fetch = matches!(&expr, Expr::FunctionCall {name,..} if name.as_str()=="__fastdb_fetch");
                    fetched.push(is_fetch);
                    if is_fetch {
                        if snapshot.is_some() {
                            return Err(unsupported("record::fetch in RETURNING"));
                        }
                        let Expr::FunctionCall {
                            args,
                            distinctness,
                            filter_over,
                            order_by,
                            within_group,
                            ..
                        } = &expr
                        else {
                            unreachable!();
                        };
                        if args.len() != 1
                            || distinctness.is_some()
                            || filter_over.filter_clause.is_some()
                            || filter_over.over_clause.is_some()
                            || !order_by.is_empty()
                            || !within_group.is_empty()
                        {
                            return Err(unsupported(
                                "record::fetch expects one unmodified reference",
                            ));
                        }
                        expr = *args[0].clone();
                        scope.typed(&mut expr)?;
                        typed.push(true);
                    } else if scope.preserved(&mut expr)? {
                        typed.push(true);
                    } else {
                        scope.lower(&mut expr)?;
                        typed.push(false);
                    }
                    names.push(name.clone());
                    rewritten.push(ResultColumn::Expr(
                        Box::new(expr),
                        Some(As::As(Name::from_string(quote(&name)))),
                    ));
                }
            }
        }
        for (i, name) in names.iter().enumerate() {
            if !positional && names[..i].contains(name) {
                return Err(Error::Validation(
                    "duplicate projection names; use AS".into(),
                ));
            }
        }
        *scope.fetched_aliases.borrow_mut() = names
            .iter()
            .zip(&fetched)
            .filter(|(_, fetch)| **fetch)
            .map(|(name, _)| name.to_ascii_lowercase())
            .collect();
        *columns = rewritten;
        if scope.sources.is_empty() {
            for ((name, column), typed) in names.iter().zip(columns.iter()).zip(&typed) {
                if scope
                    .fetched_aliases
                    .borrow()
                    .contains(&name.to_ascii_lowercase())
                {
                    continue;
                }
                if let ResultColumn::Expr(value, _) = column {
                    scope
                        .standalone_aliases
                        .borrow_mut()
                        .entry(name.to_ascii_lowercase())
                        .or_insert_with(|| (*value.clone(), *typed));
                }
            }
        }
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
            if let Some(snapshot) = snapshot {
                let doc = Value::Object(snapshot.cloned().unwrap_or_default());
                let id = snapshot
                    .and_then(|d| d.get("id"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let hex = |v: &Value| -> Result<String> {
                    Ok(v.encode()?.iter().map(|b| format!("{b:02x}")).collect())
                };
                let Cmd::Stmt(Stmt::Select(source)) = parsed(&format!(
                    "SELECT X'{}' AS doc, X'{}' AS id{}",
                    hex(&doc)?,
                    hex(&id)?,
                    if snapshot.is_none() { " WHERE 0" } else { "" }
                ))?
                else {
                    unreachable!("snapshot SELECT");
                };
                from.select = Box::new(SelectTable::Select(
                    source,
                    Some(As::As(Name::from_string(quote(&scope.sources[0].alias)))),
                ));
            } else {
                lower_source(&mut from.select, &scope.sources[0], candidates[0].clone())?;
            }
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
        if let Some(group) = group_by {
            // Ordinals refer to the original expression: grouping the encoded
            // projection would give different numeric/null SQL semantics.
            for expr in &mut group.exprs {
                let ordinal = expand_group_position(expr, &original_columns)?;
                if !ordinal && !scope.sources.is_empty() {
                    reject_group_aliases(expr, &original_columns)?;
                }
                scope.lower(expr)?;
            }
            if let Some(expr) = &mut group.having {
                if !scope.sources.is_empty() {
                    for ((name, column), typed) in names.iter().zip(columns.iter()).zip(&typed) {
                        if scope
                            .fetched_aliases
                            .borrow()
                            .contains(&name.to_ascii_lowercase())
                        {
                            continue;
                        }
                        if let ResultColumn::Expr(value, _) = column {
                            scope
                                .standalone_aliases
                                .borrow_mut()
                                .entry(name.to_ascii_lowercase())
                                .or_insert_with(|| (*value.clone(), *typed));
                        }
                    }
                }
                scope.lower(expr)?;
                // Window source expressions retain their own name-resolution scope.
                if !scope.sources.is_empty() {
                    scope.standalone_aliases.borrow_mut().clear();
                }
            }
        }
        for definition in window_clause {
            scope.lower_window(&mut definition.window)?;
        }
        // ORDER BY aliases resolve to projected values after WHERE/GROUP/window
        // lowering, including references nested inside arithmetic or helpers.
        scope.standalone_aliases.borrow_mut().clear();
        for (i, name) in names.iter().enumerate() {
            if fetched[i] {
                continue;
            }
            let reference = if distinct {
                format!("__fastdb_out_{i}")
            } else {
                name.clone()
            };
            scope
                .standalone_aliases
                .borrow_mut()
                .entry(name.to_ascii_lowercase())
                .or_insert_with(|| (Expr::Id(Name::exact(reference)), typed[i]));
        }
        let mut order_outputs = Vec::new();
        for sorted in &mut select.order_by {
            // Aliases refer to the original expression, not the encoded typed
            // projection, so sorting keeps SQL scalar semantics.
            let position = projection_position(&sorted.expr);
            if position.is_some_and(|i| i == 0 || i > columns.len()) {
                return Err(Error::Validation("ORDER BY position out of range".into()));
            }
            let alias_index = match order_base(&sorted.expr) {
                Expr::Id(n) | Expr::Name(n) => names
                    .iter()
                    .position(|name| name.eq_ignore_ascii_case(n.as_str())),
                _ => position
                    .filter(|i| *i > 0 && *i <= columns.len())
                    .map(|i| i - 1),
            };
            let alias_index = alias_index.or_else(|| original_columns.iter().position(|column| {
                matches!(column, ResultColumn::Expr(expr, _) if expr.as_ref() == order_base(&sorted.expr))
            }));
            order_outputs.push(alias_index);
            if let Some(i) = alias_index {
                if fetched[i] {
                    return Err(unsupported("ordering on fetched values"));
                }
                if distinct && !typed[i] {
                    let ResultColumn::Expr(expr, _) = &columns[i] else {
                        unreachable!("rewritten projection")
                    };
                    replace_order_base(&mut sorted.expr, *expr.clone());
                }
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
                    if !matches!(&e, Expr::FunctionCall { .. }) {
                        e = expression(&format!("__fastdb_sort_encoded({e})"))?;
                    }
                    replace_order_base(&mut sorted.expr, e);
                }
            } else if let Some((i, path)) = scope.field(&sorted.expr)? {
                let mut e = scope.accessor(i, &path, true)?;
                if let Expr::FunctionCall { name, .. } = &mut e {
                    *name = Name::exact("__fastdb_sort".into());
                }
                sorted.expr = Box::new(e);
            } else if scope.preserved(&mut sorted.expr)? {
                sorted.expr = Box::new(expression(&format!(
                    "__fastdb_sort_encoded({})",
                    sorted.expr
                ))?);
            } else {
                scope.lower(&mut sorted.expr)?;
            }
        }
        if distinct {
            if fetched.iter().any(|v| *v) {
                return Err(unsupported("DISTINCT on fetched documents"));
            }
            lower_distinct(select, &typed, &order_outputs)?;
        }
        let lowered = cmd.to_string();
        let mut statement = self.engine.prepare(&lowered)?;
        for (name, value) in params {
            let Some(index) = crate::bind_index(&statement, name) else {
                if ignore_unused || scope.consumed.borrow().contains(name) {
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
        for row in crate::collect_rows(&mut statement)? {
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
        if !explain && fetched.iter().any(|v| *v) {
            let refs = rows
                .iter()
                .flat_map(|row| {
                    row.iter()
                        .zip(&fetched)
                        .filter(|(_, fetch)| **fetch)
                        .map(|(value, _)| value.clone())
                })
                .collect::<Vec<_>>();
            let mut values = self.fetch_records(&refs)?.into_iter();
            for row in &mut rows {
                for (value, fetch) in row.iter_mut().zip(&fetched) {
                    if *fetch {
                        *value = values.next().expect("matching fetch count");
                    }
                }
            }
        }
        Ok(Some(QueryResult {
            columns: if explain { engine_names } else { names },
            rows,
            affected: 0,
        }))
    }
}

// Match the pinned engine's replace_column_number_with_copy_of_column_expr:
// COLLATE/parentheses wrap an ordinal, but only a single sign directly on a
// numeric literal counts. In particular, +(+1) and -( -1) remain expressions.
fn expand_group_position(expr: &mut Expr, columns: &[ResultColumn]) -> Result<bool> {
    match expr {
        Expr::Collate(inner, _) => return expand_group_position(inner, columns),
        Expr::Parenthesized(exprs) if exprs.len() == 1 => {
            return expand_group_position(&mut exprs[0], columns);
        }
        _ => {}
    }
    let Some(number) = projection_position(expr) else {
        return Ok(false);
    };
    if number == 0 || number > columns.len() {
        return Err(Error::Validation("GROUP BY position out of range".into()));
    }
    let ResultColumn::Expr(original, _) = &columns[number - 1] else {
        return Err(unsupported("GROUP BY document star"));
    };
    // A constant projected integer must not become another ordinal when the
    // generated query is parsed by the engine. Retain surrounding COLLATE.
    *expr = expression(&format!("coalesce({original}, NULL)"))?;
    Ok(true)
}

fn reject_group_aliases(expr: &Expr, columns: &[ResultColumn]) -> Result<()> {
    for column in columns {
        let ResultColumn::Expr(original, Some(alias)) = column else {
            continue;
        };
        if !alias.is_explicit() {
            continue;
        }
        // An alias identical to its simple field needs no substitution.
        if matches!(original.as_ref(), Expr::Id(n) | Expr::Name(n) | Expr::Qualified(_, n) | Expr::DoublyQualified(_, _, n) if n.as_str().eq_ignore_ascii_case(alias.name().as_str()))
        {
            continue;
        }
        let mut copy = expr.clone();
        let mut found = false;
        turso_core::walk_expr_mut(&mut copy, &mut |expr| {
            if matches!(expr, Expr::FunctionCall {name,..} if name.as_str()=="__fastdb_path") {
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            if matches!(expr, Expr::Id(name) | Expr::Name(name) if name.as_str().eq_ignore_ascii_case(alias.name().as_str()))
            {
                found = true;
            }
            Ok(turso_core::WalkControl::Continue)
        })?;
        if found {
            return Err(unsupported(
                "projection aliases in GROUP BY; repeat or qualify the source expression",
            ));
        }
    }
    Ok(())
}

// Prepare, but do not execute, a native star query so views, generated columns,
// hidden columns and quoted names follow the pinned engine's own expansion.
fn expand_stars(
    connection: &Connection,
    scope: &Scope,
    columns: &[ResultColumn],
) -> Result<Vec<ResultColumn>> {
    let mut expanded = Vec::new();
    for column in columns {
        let sources: Vec<&Source> = match column {
            ResultColumn::Star => scope.sources.iter().collect(),
            ResultColumn::TableStar(name) => vec![scope
                .sources
                .iter()
                .find(|s| s.alias.eq_ignore_ascii_case(name.as_str()))
                .ok_or_else(|| Error::Validation("unknown star qualifier".into()))?],
            _ => {
                expanded.push(column.clone());
                continue;
            }
        };
        if sources.is_empty() {
            return Err(Error::Validation("star requires a source".into()));
        }
        for source in sources {
            if source.collection.is_some() {
                expanded.push(ResultColumn::TableStar(Name::from_string(quote(
                    &source.alias,
                ))));
                continue;
            }
            let Cmd::Stmt(Stmt::Select(mut probe)) = parsed("SELECT * FROM placeholder")? else {
                unreachable!()
            };
            let OneSelect::Select {
                from: Some(from), ..
            } = &mut probe.body.select
            else {
                unreachable!()
            };
            from.select = Box::new(source.table.clone());
            let statement = connection
                .engine
                .prepare(Cmd::Stmt(Stmt::Select(probe)).to_string())?;
            for i in 0..statement.num_columns() {
                let name = statement.get_column_name(i).into_owned();
                expanded.push(ResultColumn::Expr(
                    Box::new(expression(&format!(
                        "{}.{}",
                        quote(&source.alias),
                        quote(&name)
                    ))?),
                    Some(As::As(Name::from_string(quote(&name)))),
                ));
            }
        }
    }
    Ok(expanded)
}
