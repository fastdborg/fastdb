//! Logical collection SELECT lowering into the pinned SQLite AST.
use crate::{quote, Collection, Connection, Error, Parameters, QueryResult, Result, Value};
use turso_parser::{ast::*, parser::Parser};

#[derive(Clone)]
struct Source {
    table: SelectTable,
    alias: String,
    collection: Option<Collection>,
    derived: Option<Vec<(String, bool)>>,
    derived_logical: bool,
    derived_physical: Option<Vec<String>>,
    native_collations: std::collections::BTreeMap<String, String>,
    native_expression_collations: std::collections::BTreeMap<String, String>,
    consumed: std::collections::BTreeSet<String>,
}
impl Source {
    fn logical(&self) -> bool {
        self.collection.is_some() || self.derived_logical
    }
    fn typed_field(&self, path: &[String]) -> bool {
        self.collection.is_some()
            || self.derived.as_ref().is_some_and(|columns| {
                let position = columns
                    .iter()
                    .position(|(name, _)| name.eq_ignore_ascii_case(&path[0]))
                    .or_else(|| {
                        self.derived_physical.as_ref().and_then(|names| {
                            names
                                .iter()
                                .position(|name| name.eq_ignore_ascii_case(&path[0]))
                        })
                    });
                position.is_some_and(|i| columns[i].1)
            })
    }
}
// Match the aggregate's case-insensitive name while preserving its argument
// AST, whose equivalence is still determined by the pinned engine.
fn aggregate_reuse_key(expr: &Expr) -> String {
    let mut expr = expr.clone();
    if let Expr::FunctionCall { name, .. } | Expr::FunctionCallStar { name, .. } = &mut expr {
        *name = Name::exact(name.as_str().to_ascii_lowercase());
    }
    expr.to_string()
}

type CteSources = std::collections::BTreeMap<String, Option<Source>>;
#[derive(Clone, PartialEq, Eq)]
enum SubqueryAffinity {
    None,
    MembershipColumn,
    // An empty source name keeps a correlated RHS local to its membership expression.
    NativeMembership(String, String),
    NativeScalar(String),
}
// Prepare scalar metadata without evaluating its expressions. An unresolved
// outer column denotes a correlated scalar register, whose collation is BINARY.
fn native_scalar_collation(
    connection: &Connection,
    metadata_scopes: &[With],
    inner: &Select,
    local: &std::collections::BTreeSet<String>,
) -> Result<String> {
    let mut sql = Cmd::Stmt(Stmt::Select(inner.clone())).to_string();
    for scope in metadata_scopes.iter().rev() {
        sql = format!(
            "{scope} SELECT * FROM ({})",
            sql.trim().trim_end_matches(';')
        );
    }
    let statement = match connection.prepare(sql) {
        Ok(statement) => statement,
        Err(error) if error.to_string().contains("no such column:") => return Ok("BINARY".into()),
        Err(error)
            if error
                .to_string()
                .split_once("no such table: ")
                .is_some_and(|(_, alias)| {
                    local.contains(&alias.trim().trim_matches('"').to_ascii_lowercase())
                }) =>
        {
            return Ok("BINARY".into())
        }
        Err(error) => return Err(error.into()),
    };
    let program = statement.get_program();
    let mut implicit = None;
    let mut explicit = None;
    if let Some(column) = program.result_columns.first() {
        let mut value = column.expr.clone();
        turso_core::walk_expr_mut(&mut value, &mut |expr| {
            match expr {
                Expr::Collate(_, name) => {
                    explicit.get_or_insert_with(|| name.as_str().to_owned());
                    return Ok(turso_core::WalkControl::SkipChildren);
                }
                Expr::Column { table, column, .. } => {
                    if let Some((_, source)) =
                        program.table_references.find_table_by_internal_id(*table)
                    {
                        if let Some(column) = source.get_column_at(*column) {
                            implicit.get_or_insert_with(|| column.collation().name());
                        }
                    }
                }
                _ => {}
            }
            Ok(turso_core::WalkControl::Continue)
        })?;
    }
    Ok(explicit.or(implicit).unwrap_or_else(|| "BINARY".into()))
}
// Grouping can change the row selected by unordered distinct-set pagination.
// UNION ALL uses direct arms so the engine can skip projections before OFFSET.
fn direct_compound_pagination(select: &Select) -> bool {
    fn direct(value: &Expr) -> bool {
        match value {
            Expr::Literal(Literal::Numeric(_)) | Expr::Variable(_) => true,
            Expr::Unary(_, value) => direct(value),
            _ => false,
        }
    }
    select.limit.as_ref().is_some_and(|limit| {
        direct(&limit.expr) && limit.offset.as_ref().is_none_or(|offset| direct(offset))
    })
}

fn direct_source_free_membership(select: &Select) -> bool {
    select.with.is_none()
        && !select.body.compounds.is_empty()
        && select.order_by.is_empty()
        && direct_compound_pagination(select)
        && std::iter::once(&select.body.select)
            .chain(select.body.compounds.iter().map(|arm| &arm.select))
            .all(|arm| matches!(arm, OneSelect::Select { from: None, .. }))
}

fn unordered_compound_pagination(select: &Select) -> bool {
    select.limit.is_some()
        && select.order_by.is_empty()
        && (!direct_compound_pagination(select)
            || select
                .body
                .compounds
                .iter()
                .any(|arm| arm.operator != CompoundOperator::UnionAll))
}

fn resolve_compound_expression_order_names(select: &mut Select) {
    if select.body.compounds.is_empty() {
        return;
    }
    for sorted in &mut select.order_by {
        let (Expr::Id(name) | Expr::Name(name)) = order_base(&sorted.expr) else {
            continue;
        };
        let Ok(normalized) = expression(name.as_str()) else {
            continue;
        };
        let position = std::iter::once(&select.body.select)
            .chain(select.body.compounds.iter().map(|arm| &arm.select))
            .find_map(|arm| {
                let OneSelect::Select { columns, .. } = arm else {
                    return None;
                };
                if columns
                    .iter()
                    .any(|column| !matches!(column, ResultColumn::Expr(_, _)))
                {
                    return None;
                }
                // The pinned resolver checks aliases before inferred names in
                // each arm, then advances to the next arm.
                columns
                    .iter()
                    .position(|column| {
                        matches!(column,
                    ResultColumn::Expr(_, Some(alias))
                        if alias.name().as_str().eq_ignore_ascii_case(name.as_str()))
                    })
                    .or_else(|| {
                        columns.iter().position(|column| {
                            matches!(column,
                        ResultColumn::Expr(value, Some(As::ImplicitColumnName(_)))
                            if value.to_string() == normalized.to_string())
                        })
                    })
            });
        if let Some(position) = position {
            replace_order_base(
                &mut sorted.expr,
                Expr::Literal(Literal::Numeric((position + 1).to_string())),
            );
        }
    }
}

fn preserve_compound_column_names(select: &mut Select) {
    // ImplicitColumnName is parser metadata and is omitted when serializing.
    // Make it explicit before correlation replaces the original expression.
    if let OneSelect::Select { columns, .. } = &mut select.body.select {
        for column in columns {
            if let ResultColumn::Expr(_, Some(alias @ As::ImplicitColumnName(_))) = column {
                *alias = As::As(Name::from_string(quote(alias.name().as_str())));
            }
        }
    }
}

// Predicate-only correlation leaves the native projection intact.
// The probe substitutes NULL solely for metadata preparation; the executable
// query retains its outer references and is evaluated by the engine per row.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NativeCorrelationMode {
    Native,
    ScalarPagination,
    Membership,
    // Membership compares native values inside each arm before set operations.
    CompoundArm,
}

// Match the pinned MustBeInt boundary without executing SQL during planning.
fn pagination_integer(value: &Value) -> Option<i64> {
    fn real(number: f64) -> Option<i64> {
        let integer = number as i64;
        (number.is_finite()
            && integer != i64::MIN
            && integer != i64::MAX
            && integer as f64 == number)
            .then_some(integer)
    }
    match value {
        Value::Integer(number) => Some(*number),
        Value::Boolean(value) => Some(i64::from(*value)),
        Value::Number(number) => real(*number),
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty()
                || text.starts_with(['e', 'E'])
                || text.starts_with(".e")
                || text.starts_with(".E")
            {
                return None;
            }
            let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
            let mut parts = unsigned.split(['e', 'E']);
            let mantissa = parts.next()?;
            let exponent = parts.next();
            if parts.next().is_some() {
                return None;
            }
            if let Some(exponent) = exponent {
                let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
                if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                    return None;
                }
            }
            if mantissa.bytes().filter(|byte| *byte == b'.').count() > 1
                || !mantissa
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'.')
                || (mantissa.is_empty() && exponent.is_none())
            {
                return None;
            }
            if exponent.is_none() && !mantissa.contains('.') {
                text.parse().ok()
            } else {
                // Pinned numeric conversion treats malformed signed decimal
                // forms such as '+.' as zero after lexical acceptance.
                real(text.parse::<f64>().unwrap_or(0.0))
            }
        }
        _ => None,
    }
}

// Keep direct scalar pagination when its values can be represented literally.
// Other value types and expressions retain the existing validation path.
fn literal_pagination(value: &mut Expr, params: &Parameters) -> Result<bool> {
    match value {
        Expr::Literal(Literal::Numeric(_)) => Ok(true),
        Expr::Unary(_, value) => literal_pagination(value, params),
        Expr::Variable(var) => {
            let name = var
                .name
                .as_ref()
                .map_or_else(|| format!("?{}", var.index), |name| name.to_string());
            let bound = params
                .get(&name)
                .ok_or_else(|| Error::Parameter(name.clone()))?;
            if let Some(number) = pagination_integer(bound) {
                *value = expression(&number.to_string())?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        _ => Ok(false),
    }
}

fn native_correlated_predicate(
    connection: &Connection,
    metadata_scopes: &[With],
    inner: &Select,
    sources: &[Source],
    metadata: bool,
    params: &Parameters,
    mode: NativeCorrelationMode,
) -> Result<(Select, bool)> {
    let mut scopes = metadata_scopes.to_vec();
    if let Some(with) = &inner.with {
        if with.recursive {
            return Ok((inner.clone(), false));
        }
        scopes.push(with.clone());
    }
    native_correlated_body(connection, &scopes, inner, sources, metadata, params, mode)
}

fn native_correlated_body(
    connection: &Connection,
    metadata_scopes: &[With],
    inner: &Select,
    sources: &[Source],
    metadata: bool,
    params: &Parameters,
    mode: NativeCorrelationMode,
) -> Result<(Select, bool)> {
    let mode = if mode == NativeCorrelationMode::CompoundArm && !sources.iter().any(Source::logical)
    {
        NativeCorrelationMode::Native
    } else {
        mode
    };
    let scalar_pagination = mode == NativeCorrelationMode::ScalarPagination;
    let mut inner = inner.clone();
    if !inner.body.compounds.is_empty() {
        resolve_compound_expression_order_names(&mut inner);
        let direct_membership = mode == NativeCorrelationMode::Membership
            && direct_compound_pagination(&inner)
            && inner.order_by.is_empty();
        if unordered_compound_pagination(&inner) && !direct_membership {
            return Ok((inner, false));
        }
        for arm in std::iter::once(&mut inner.body.select)
            .chain(inner.body.compounds.iter_mut().map(|arm| &mut arm.select))
        {
            let mut single = Select {
                with: None,
                body: SelectBody {
                    select: arm.clone(),
                    compounds: Vec::new(),
                },
                order_by: Vec::new(),
                limit: None,
            };
            preserve_compound_column_names(&mut single);
            let (prepared, logical) = native_correlated_predicate(
                connection,
                metadata_scopes,
                &single,
                sources,
                metadata,
                params,
                if direct_membership {
                    NativeCorrelationMode::CompoundArm
                } else {
                    NativeCorrelationMode::Native
                },
            )?;
            if logical && !metadata {
                return Err(unsupported("correlated logical compound projection"));
            }
            *arm = prepared.body.select;
        }
        return Ok((inner, false));
    }
    let OneSelect::Select {
        columns,
        from,
        where_clause,
        group_by,
        ..
    } = &mut inner.body.select
    else {
        return Ok((inner, false));
    };
    let mut local = std::collections::BTreeSet::new();
    if let Some(from) = from {
        for table in std::iter::once(&from.select).chain(from.joins.iter().map(|j| &j.table)) {
            match table.as_ref() {
                SelectTable::Table(name, alias, _) => {
                    local.insert(
                        alias
                            .as_ref()
                            .map_or(name.name.as_str(), |a| a.name().as_str())
                            .to_ascii_lowercase(),
                    );
                }
                SelectTable::Select(_, alias) => {
                    if let Some(alias) = alias {
                        local.insert(alias.name().as_str().to_ascii_lowercase());
                    }
                }
                _ => return Ok((inner, false)),
            }
        }
    }
    let outer_sources: Vec<_> = sources
        .iter()
        .filter(|source| !local.contains(&source.alias.to_ascii_lowercase()))
        .cloned()
        .collect();
    fn qualifier(expr: &Expr) -> Option<&str> {
        match expr {
            Expr::Qualified(alias, _) | Expr::DoublyQualified(alias, _, _) => Some(alias.as_str()),
            Expr::FieldAccess { base, .. } => qualifier(base),
            Expr::FunctionCall { name, args, .. } if name.as_str() == "__fastdb_path" => {
                match args.first()?.as_ref() {
                    Expr::Id(alias) | Expr::Name(alias) => Some(alias.as_str()),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    let correlation_aliases = local
        .iter()
        .cloned()
        .chain(
            outer_sources
                .iter()
                .map(|source| source.alias.to_ascii_lowercase()),
        )
        .collect();
    let rewrite = |value: &mut Expr, typed: bool| -> Result<bool> {
        let mut correlated = false;
        // Lowering the enclosing predicate must preserve nested native queries,
        // while recursively binding any qualified outer references they contain.
        let mut nested_queries = ExpressionSubqueries::new();
        let mut nested_error = None;
        turso_core::walk_expr_mut(value, &mut |expr| {
            if matches!(expr, Expr::InSelect { .. }) {
                let original = expr.to_string();
                let Expr::InSelect { rhs, .. } = expr else {
                    unreachable!()
                };
                let result = (|| -> Result<_> {
                    let (prepared, logical) = native_correlated_predicate(
                        connection,
                        metadata_scopes,
                        rhs,
                        &outer_sources,
                        metadata,
                        params,
                        NativeCorrelationMode::Membership,
                    )?;
                    let changed = prepared.to_string() != rhs.to_string();
                    if metadata {
                        return Ok((prepared, None, changed));
                    }
                    let (query, affinity) = if logical {
                        let column = matches!(&rhs.body.select, OneSelect::Select { columns, .. } if matches!(columns.as_slice(), [ResultColumn::Expr(value, _)] if membership_column(value)));
                        let id = connection
                            .next_subquery_id
                            .fetch_update(
                                std::sync::atomic::Ordering::Relaxed,
                                std::sync::atomic::Ordering::Relaxed,
                                |id| id.checked_add(1),
                            )
                            .map_err(|_| {
                                Error::Limit("internal subquery identifiers exhausted".into())
                            })?;
                        let name = format!("__fastdb_nested_members_{id}");
                        let sql = Cmd::Stmt(Stmt::Select(prepared.clone())).to_string();
                        (
                            expression(&format!(
                                "(WITH {name}(v) AS ({}) SELECT __fastdb_unwrap(v) FROM {name})",
                                sql.trim().trim_end_matches(';')
                            ))?,
                            if column {
                                SubqueryAffinity::MembershipColumn
                            } else {
                                SubqueryAffinity::None
                            },
                        )
                    } else {
                        let collation = native_scalar_collation(
                            connection,
                            metadata_scopes,
                            rhs,
                            &correlation_aliases,
                        )?;
                        (
                            Expr::Subquery(prepared.clone()),
                            SubqueryAffinity::NativeMembership(collation, String::new()),
                        )
                    };
                    Ok((
                        prepared,
                        Some((query, Default::default(), affinity)),
                        changed,
                    ))
                })();
                match result {
                    Ok((prepared, plan, changed)) => {
                        correlated |= changed;
                        if metadata {
                            *rhs = prepared;
                        }
                        if let Some(plan) = plan {
                            nested_queries.insert(original, plan);
                        }
                    }
                    Err(error) => nested_error = Some(error),
                }
                // The core walker visits the LHS, but does not traverse SELECT RHSs.
                return Ok(turso_core::WalkControl::Continue);
            }

            if matches!(expr, Expr::Exists(_) | Expr::Subquery(_)) {
                let original = expr.to_string();
                let scalar = matches!(expr, Expr::Subquery(_));
                let (Expr::Exists(inner) | Expr::Subquery(inner)) = expr else {
                    unreachable!()
                };
                let (prepared, typed_projection) = match native_correlated_predicate(
                    connection,
                    metadata_scopes,
                    inner,
                    &outer_sources,
                    metadata,
                    params,
                    if scalar {
                        NativeCorrelationMode::ScalarPagination
                    } else {
                        NativeCorrelationMode::Native
                    },
                ) {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        nested_error = Some(error);
                        return Ok(turso_core::WalkControl::SkipChildren);
                    }
                };
                let scalar_collation = if scalar && !metadata && !typed_projection {
                    match native_scalar_collation(
                        connection,
                        metadata_scopes,
                        inner,
                        &correlation_aliases,
                    ) {
                        Ok(collation) => collation,
                        Err(error) => {
                            nested_error = Some(error);
                            return Ok(turso_core::WalkControl::SkipChildren);
                        }
                    }
                } else {
                    "BINARY".into()
                };
                let prepared = if scalar {
                    Expr::Subquery(prepared)
                } else {
                    Expr::Exists(prepared)
                };
                correlated |= prepared.to_string() != original;
                if metadata {
                    *expr = prepared;
                } else {
                    let (prepared, affinity) = if scalar && !typed_projection {
                        match expression(&format!("__fastdb_pack({prepared})")) {
                            Ok(prepared) => {
                                (prepared, SubqueryAffinity::NativeScalar(scalar_collation))
                            }
                            Err(error) => {
                                nested_error = Some(error);
                                return Ok(turso_core::WalkControl::SkipChildren);
                            }
                        }
                    } else {
                        (prepared, SubqueryAffinity::None)
                    };
                    nested_queries.insert(original, (prepared, Default::default(), affinity));
                }
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            if matches!(expr, Expr::Subquery(_) | Expr::InSelect { .. }) {
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            if typed {
                if let Expr::Variable(var) = expr {
                    let name = var
                        .name
                        .as_ref()
                        .map_or_else(|| format!("?{}", var.index), |name| name.to_string());
                    correlated |= params.get(&name).is_some_and(|value| {
                        matches!(
                            value,
                            Value::Boolean(_)
                                | Value::Record(_)
                                | Value::Array(_)
                                | Value::Object(_)
                                | Value::Vector(_)
                        )
                    });
                }
            }
            if let Some(alias) = qualifier(expr) {
                if !local.contains(&alias.to_ascii_lowercase())
                    && sources.iter().any(|s| s.alias.eq_ignore_ascii_case(alias))
                {
                    correlated = true;
                    if metadata {
                        *expr = Expr::Literal(Literal::Null);
                    }
                }
            }
            Ok(turso_core::WalkControl::Continue)
        })?;
        if let Some(error) = nested_error {
            return Err(error);
        }
        if (correlated || mode == NativeCorrelationMode::CompoundArm) && !metadata {
            let scope = Scope {
                expression_subqueries: nested_queries,
                using: UsingColumns::default(),
                qualified_only: true,
                sources: outer_sources.clone(),
                params: params.clone(),
                consumed: Default::default(),
                fetched_aliases: Default::default(),
                standalone_aliases: Default::default(),
            };
            if typed && mode == NativeCorrelationMode::CompoundArm {
                if !scope.comparison_key(value)? {
                    scope.lower(value)?;
                    *value = expression(&format!("__fastdb_unwrap(__fastdb_pack({value}))"))?;
                }
            } else if typed {
                scope.typed(value)?;
            } else {
                scope.lower(value)?;
            }
        }
        Ok(correlated)
    };
    let mut correlated_query = false;
    if let Some(value) = where_clause {
        correlated_query |= rewrite(value, false)?;
    }
    if let Some(value) = group_by.as_mut().and_then(|group| group.having.as_mut()) {
        correlated_query |= rewrite(value, false)?;
    }
    if let Some(from) = from {
        for join in &mut from.joins {
            if let Some(JoinConstraint::On(value)) = &mut join.constraint {
                correlated_query |= rewrite(value, false)?;
            }
        }
    }
    fn has_cast_affinity(value: &Expr) -> bool {
        match value {
            Expr::Cast { .. } => true,
            Expr::Collate(value, _) => has_cast_affinity(value),
            Expr::Parenthesized(values) if values.len() == 1 => has_cast_affinity(&values[0]),
            _ => false,
        }
    }
    let mut typed_projection = false;
    let mut typed_sort_values = Vec::new();
    for (index, column) in columns.iter_mut().enumerate() {
        if let ResultColumn::Expr(value, alias) = column {
            // Explicit casts produce native scalars whose affinity must remain
            // attached to the subquery result in outer comparisons.
            let mut typed = mode == NativeCorrelationMode::CompoundArm || !has_cast_affinity(value);
            let correlated = rewrite(value, typed)?;
            correlated_query |= correlated;
            if correlated && typed && !metadata {
                if let Expr::FunctionCall { name, args, .. } = value.as_ref() {
                    if name.as_str() == "__fastdb_pack" && args.len() == 1 {
                        // This projection is already a native scalar. Keep the
                        // engine's alias reuse and pack only the scalar result.
                        *value = args[0].clone();
                        typed = false;
                    }
                }
            }
            typed_projection |= correlated && typed;
            if correlated && typed && !metadata {
                typed_sort_values.push((index + 1, alias.clone(), value.clone()));
            }
        }
    }
    fn sort_base(value: &Expr) -> &Expr {
        match value {
            Expr::Collate(value, _) => sort_base(value),
            Expr::Parenthesized(values) if values.len() == 1 => sort_base(&values[0]),
            _ => value,
        }
    }
    let typed_sort_arguments = typed_sort_values
        .iter()
        .map(|(_, alias, value)| {
            Ok((
                alias.clone(),
                expression(&format!("__fastdb_unwrap({value})"))?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let single_projection = columns.len() == 1;
    let mut selected_sorts = 0;
    let mut sort_selections = Vec::new();
    for sorted in &mut inner.order_by {
        let selected =
            typed_sort_values
                .iter()
                .find(|(index, alias, _)| match sort_base(&sorted.expr) {
                    Expr::Literal(Literal::Numeric(n)) => n.parse::<usize>().ok() == Some(*index),
                    Expr::Id(name) | Expr::Name(name) => alias
                        .as_ref()
                        .is_some_and(|a| a.name().as_str().eq_ignore_ascii_case(name.as_str())),
                    _ => false,
                });
        if let Some((_, _, value)) = selected {
            selected_sorts += 1;
            sort_selections.push(true);
            let replacement = expression(&format!("__fastdb_unwrap({value})"))?;
            replace_order_base(&mut sorted.expr, replacement);
            continue;
        }
        sort_selections.push(false);
        correlated_query |= rewrite(&mut sorted.expr, false)?;
        // The pinned engine resolves projection aliases inside ORDER BY
        // expressions before same-named input columns. Preserve that binding
        // while exposing logical scalar values to arithmetic/functions.
        turso_core::walk_expr_mut(&mut sorted.expr, &mut |expr| {
            if matches!(
                expr,
                Expr::Subquery(_) | Expr::Exists(_) | Expr::InSelect { .. }
            ) {
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            if let Expr::Id(name) | Expr::Name(name) = expr {
                if let Some((_, value)) = typed_sort_arguments.iter().find(|(alias, _)| {
                    alias.as_ref().is_some_and(|alias| {
                        alias.name().as_str().eq_ignore_ascii_case(name.as_str())
                    })
                }) {
                    *expr = value.clone();
                    return Ok(turso_core::WalkControl::SkipChildren);
                }
            }
            Ok(turso_core::WalkControl::Continue)
        })?;
    }
    let typed_distinct = !typed_sort_values.is_empty()
        && matches!(
            &inner.body.select,
            OneSelect::Select {
                distinctness: Some(Distinctness::Distinct),
                ..
            }
        );
    if single_projection && (selected_sorts > 0 || typed_distinct) {
        // Keep the projected value available to sorting without evaluating it
        // twice. OFFSET prevents flattening; a lazy CTE preserves LIMIT 0.
        // DISTINCT belongs outside this boundary so hidden sort keys cannot
        // turn duplicate projected values into distinct rows.
        let mut ordering = std::mem::take(&mut inner.order_by);
        let limit = inner.limit.take();
        let mut names = vec!["v".to_owned()];
        let mut projected_distinctness = None;
        if let OneSelect::Select {
            columns,
            distinctness,
            ..
        } = &mut inner.body.select
        {
            projected_distinctness = distinctness.take();
            for (index, (sorted, selected)) in ordering.iter_mut().zip(&sort_selections).enumerate()
            {
                if !selected {
                    let name = format!("k{index}");
                    columns.push(ResultColumn::Expr(sorted.expr.clone(), None));
                    names.push(name.clone());
                    sorted.expr = Box::new(expression(&name)?);
                }
            }
        }
        let names = names.join(",");
        let sql = Cmd::Stmt(Stmt::Select(inner)).to_string();
        let mut suffix = 0;
        let name = loop {
            let name = format!("__fastdb_sorted_projection_{suffix}");
            if !sql.to_ascii_lowercase().contains(&name) {
                break name;
            }
            suffix += 1;
        };
        let Expr::Subquery(mut wrapped) = expression(&format!(
            "(WITH {name}({names}) AS NOT MATERIALIZED ({} LIMIT -1 OFFSET 0) SELECT v FROM {name})",
            sql.trim().trim_end_matches(';')
        ))?
        else {
            unreachable!()
        };
        for (sorted, selected) in ordering.iter_mut().zip(&sort_selections) {
            if *selected {
                replace_order_base(&mut sorted.expr, expression("__fastdb_unwrap(v)")?);
            }
        }
        if let OneSelect::Select {
            distinctness,
            group_by,
            ..
        } = &mut wrapped.body.select
        {
            if matches!(projected_distinctness, Some(Distinctness::Distinct)) {
                // Retain a typed representative, but compare logical SQL
                // values so integer/real equivalents form one distinct row.
                *group_by = Some(GroupBy {
                    exprs: vec![Box::new(expression("__fastdb_unwrap(v)")?)],
                    having: None,
                });
            } else {
                *distinctness = projected_distinctness;
            }
        }
        wrapped.order_by = ordering;
        wrapped.limit = limit;
        inner = wrapped;
    }
    // A derived pagination wrapper changes ordered local-CTE correlation on
    // the pinned engine. Preserve direct integer pagination for this scope.
    let direct_local_pagination = if inner.with.is_some() && !metadata {
        if let Some(mut limit) = inner.limit.clone() {
            let direct = literal_pagination(&mut limit.expr, params)?
                && match &mut limit.offset {
                    Some(offset) => literal_pagination(offset, params)?,
                    None => true,
                };
            if direct {
                inner.limit = Some(limit);
            }
            direct
        } else {
            false
        }
    } else {
        false
    };
    if correlated_query && !metadata && !direct_local_pagination {
        if let Some(limit) = &mut inner.limit {
            for value in std::iter::once(&mut limit.expr).chain(limit.offset.iter_mut()) {
                **value = expression(&format!("__fastdb_pagination_value({value})"))?;
            }
        }
    }
    // Keep pagination on a relation: the pinned scalar-subquery compiler
    // otherwise replaces a bound LIMIT with its implicit one-row limit.
    if correlated_query
        && !metadata
        && !direct_local_pagination
        && inner.limit.is_some()
        && (scalar_pagination || selected_sorts > 0)
    {
        let sql = Cmd::Stmt(Stmt::Select(inner)).to_string();
        let Expr::Subquery(paginated) = expression(&format!(
            "(SELECT * FROM ({}) LIMIT -1 OFFSET 0)",
            sql.trim().trim_end_matches(';')
        ))?
        else {
            unreachable!()
        };
        inner = paginated;
    }
    Ok((
        inner,
        typed_projection && mode != NativeCorrelationMode::CompoundArm,
    ))
}
// Each entry stores lowered SQL, consumed binds, and affinity provenance.
type ExpressionSubqueries = std::collections::BTreeMap<
    String,
    (Expr, std::collections::BTreeSet<String>, SubqueryAffinity),
>;
#[derive(Clone, Default)]
struct UsingColumns {
    bindings: std::collections::BTreeMap<String, (usize, String)>,
    hidden: std::collections::BTreeSet<(usize, String)>,
}
// Rename one local source and the qualified references bound to it. Work on a
// clone: unsupported scopes must leave the original query intact. Quote new
// names so internal aliases cannot be mistaken for logical function markers.
fn rename_correlated_local_alias(select: &mut Select, old: &str, new: &str) -> Result<bool> {
    fn expression(value: &mut Expr, old: &str, new: &str) -> turso_core::Result<bool> {
        let mut supported = true;
        turso_core::walk_expr_mut(value, &mut |expr| {
            match expr {
                Expr::Subquery(inner) | Expr::Exists(inner) => {
                    supported &= scope(inner, old, new, false)?;
                    return Ok(turso_core::WalkControl::SkipChildren);
                }
                Expr::InSelect { lhs, rhs, .. } => {
                    supported &= expression(lhs, old, new)?;
                    supported &= scope(rhs, old, new, false)?;
                    return Ok(turso_core::WalkControl::SkipChildren);
                }
                Expr::Qualified(alias, _) if alias.as_str().eq_ignore_ascii_case(old) => {
                    *alias = Name::from_string(quote(new));
                }
                Expr::DoublyQualified(..) => supported = false,
                Expr::FunctionCall { name, .. } if name.as_str() == "__fastdb_path" => {
                    supported = false;
                }
                _ => {}
            }
            Ok(turso_core::WalkControl::Continue)
        })?;
        Ok(supported)
    }
    fn scope(select: &mut Select, old: &str, new: &str, root: bool) -> turso_core::Result<bool> {
        if select.with.is_some() {
            return Ok(false);
        }
        if !select.body.compounds.is_empty() {
            if root
                || (unordered_compound_pagination(select) && !direct_compound_pagination(select))
            {
                return Ok(false);
            }
            for arm in std::iter::once(&mut select.body.select)
                .chain(select.body.compounds.iter_mut().map(|arm| &mut arm.select))
            {
                let mut single = Select {
                    with: None,
                    body: SelectBody {
                        select: arm.clone(),
                        compounds: Vec::new(),
                    },
                    order_by: Vec::new(),
                    limit: None,
                };
                if !scope(&mut single, old, new, false)? {
                    return Ok(false);
                }
                *arm = single.body.select;
            }
            return Ok(true);
        }
        let OneSelect::Select {
            columns,
            from,
            where_clause,
            group_by,
            window_clause,
            ..
        } = &mut select.body.select
        else {
            return Ok(false);
        };
        if !window_clause.is_empty() {
            return Ok(false);
        }
        if let Some(from) = from {
            for table in std::iter::once(&from.select).chain(from.joins.iter().map(|j| &j.table)) {
                let alias = match table.as_ref() {
                    SelectTable::Table(name, alias, _) => {
                        Some(alias.as_ref().map_or(&name.name, As::name))
                    }
                    SelectTable::Select(_, alias) => alias.as_ref().map(As::name),
                    _ => return Ok(false),
                };
                if !root && alias.is_some_and(|alias| alias.as_str().eq_ignore_ascii_case(old)) {
                    // This entire SELECT body resolves the name locally.
                    return Ok(true);
                }
            }
        }
        let mut supported = true;
        for column in columns {
            match column {
                ResultColumn::Expr(value, _) => supported &= expression(value, old, new)?,
                ResultColumn::TableStar(alias) if alias.as_str().eq_ignore_ascii_case(old) => {
                    *alias = Name::from_string(quote(new));
                }
                _ => {}
            }
        }
        if let Some(from) = from {
            for table in
                std::iter::once(&mut from.select).chain(from.joins.iter_mut().map(|j| &mut j.table))
            {
                match table.as_mut() {
                    SelectTable::Table(name, alias, _) => {
                        if root
                            && alias
                                .as_ref()
                                .map_or(&name.name, As::name)
                                .as_str()
                                .eq_ignore_ascii_case(old)
                        {
                            *alias = Some(As::As(Name::from_string(quote(new))));
                        }
                    }
                    SelectTable::Select(inner, alias) => {
                        supported &= scope(inner, old, new, false)?;
                        if root
                            && alias
                                .as_ref()
                                .is_some_and(|a| a.name().as_str().eq_ignore_ascii_case(old))
                        {
                            *alias = Some(As::As(Name::from_string(quote(new))));
                        }
                    }
                    _ => return Ok(false),
                }
            }
            for join in &mut from.joins {
                if let Some(JoinConstraint::On(value)) = &mut join.constraint {
                    supported &= expression(value, old, new)?;
                }
            }
        }
        if let Some(value) = where_clause {
            supported &= expression(value, old, new)?;
        }
        if let Some(group) = group_by {
            for value in &mut group.exprs {
                supported &= expression(value, old, new)?;
            }
            if let Some(value) = &mut group.having {
                supported &= expression(value, old, new)?;
            }
        }
        for sorted in &mut select.order_by {
            supported &= expression(&mut sorted.expr, old, new)?;
        }
        if let Some(limit) = &mut select.limit {
            supported &= expression(&mut limit.expr, old, new)?;
            if let Some(offset) = &mut limit.offset {
                supported &= expression(offset, old, new)?;
            }
        }
        Ok(supported)
    }
    let mut renamed = select.clone();
    if !scope(&mut renamed, old, new, true)? {
        return Ok(false);
    }
    *select = renamed;
    Ok(true)
}
fn qualify_correlated_using(
    connection: &Connection,
    params: &Parameters,
    ctes: &CteSources,
    inner: &mut Select,
    sources: &[Source],
    using: &UsingColumns,
) -> Result<()> {
    if using.bindings.is_empty() {
        return Ok(());
    }
    if let Some(with) = &inner.with {
        if with.recursive {
            return Ok(());
        }
        let mut local_ctes = ctes.clone();
        for cte in &with.ctes {
            let Cmd::Stmt(Stmt::Select(mut probe)) =
                parsed(&format!("SELECT * FROM {}", quote(cte.tbl_name.as_str())))?
            else {
                unreachable!()
            };
            probe.with = Some(with.clone());
            let table = SelectTable::Select(probe, Some(As::As(cte.tbl_name.clone())));
            let local = source(connection, &table, params, ctes, None, String::new(), true)?;
            if local.logical() {
                return Ok(());
            }
            local_ctes.insert(cte.tbl_name.as_str().to_ascii_lowercase(), Some(local));
        }
        let with = inner.with.take();
        let result =
            qualify_correlated_using(connection, params, &local_ctes, inner, sources, using);
        inner.with = with;
        return result;
    }
    if !inner.body.compounds.is_empty() {
        resolve_compound_expression_order_names(inner);
        for arm in std::iter::once(&mut inner.body.select)
            .chain(inner.body.compounds.iter_mut().map(|arm| &mut arm.select))
        {
            let mut single = Select {
                with: None,
                body: SelectBody {
                    select: arm.clone(),
                    compounds: Vec::new(),
                },
                order_by: Vec::new(),
                limit: None,
            };
            preserve_compound_column_names(&mut single);
            qualify_correlated_using(connection, params, ctes, &mut single, sources, using)?;
            *arm = single.body.select;
        }
        return Ok(());
    }
    // Resolve only closed local source schemas here. Open collections and
    // other source forms need their own lexical scope resolution.
    // The pinned planner keeps outer merged keys available to deeper query
    // scopes even when this SELECT has a same-named local column. Local columns
    // still take precedence in this SELECT's own expressions.
    let mut inherited_using = using.clone();
    let mut using = using.clone();
    let mut local_aliases = Vec::new();
    if let OneSelect::Select {
        from: Some(from), ..
    } = &inner.body.select
    {
        for table in std::iter::once(&from.select).chain(from.joins.iter().map(|j| &j.table)) {
            if !matches!(
                table.as_ref(),
                SelectTable::Table(..) | SelectTable::Select(..)
            ) {
                return Ok(());
            }
            let local = source(connection, table, params, ctes, None, String::new(), true)?;
            let Some(columns) = local
                .derived
                .as_ref()
                .filter(|_| local.collection.is_none())
            else {
                return Ok(());
            };
            local_aliases.push(local.alias.clone());
            using.bindings.retain(|name, _| {
                !columns
                    .iter()
                    .any(|(column, _)| column.eq_ignore_ascii_case(name))
            });
        }
    }
    for alias in local_aliases {
        if inherited_using
            .bindings
            .values()
            .any(|(index, _)| sources[*index].alias.eq_ignore_ascii_case(&alias))
        {
            let id = connection
                .next_subquery_id
                .fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |id| id.checked_add(1),
                )
                .map_err(|_| Error::Limit("internal subquery identifiers exhausted".into()))?;
            if !rename_correlated_local_alias(inner, &alias, &format!("__fastdb_local_{id}"))? {
                using
                    .bindings
                    .retain(|_, (index, _)| !sources[*index].alias.eq_ignore_ascii_case(&alias));
                inherited_using
                    .bindings
                    .retain(|_, (index, _)| !sources[*index].alias.eq_ignore_ascii_case(&alias));
            }
        }
    }
    let sourceful = matches!(&inner.body.select, OneSelect::Select { from: Some(_), .. });
    let OneSelect::Select {
        columns,
        where_clause,
        group_by,
        from,
        ..
    } = &mut inner.body.select
    else {
        return Ok(());
    };
    let aliases: std::collections::BTreeSet<_> = columns
        .iter()
        .filter_map(|column| match column {
            ResultColumn::Expr(_, Some(alias)) if alias.is_explicit() => {
                Some(alias.name().as_str().to_ascii_lowercase())
            }
            _ => None,
        })
        .collect();
    for (value, preserves_aliases) in columns
        .iter_mut()
        .filter_map(|column| match column {
            ResultColumn::Expr(value, _) => Some(value),
            _ => None,
        })
        .chain(where_clause.iter_mut())
        .chain(
            from.iter_mut()
                .flat_map(|from| from.joins.iter_mut())
                .filter_map(|join| match &mut join.constraint {
                    Some(JoinConstraint::On(value)) => Some(value),
                    _ => None,
                }),
        )
        .map(|value| (value, false))
        .chain(
            group_by
                .iter_mut()
                .flat_map(|group| group.having.iter_mut())
                .map(|value| (value, true)),
        )
        .chain(
            inner
                .order_by
                .iter_mut()
                .map(|sorted| (&mut sorted.expr, true)),
        )
    {
        let mut nested_error = None;
        turso_core::walk_expr_mut(value, &mut |expr| {
            if let Expr::InSelect { rhs, .. } = expr {
                if sourceful || matches!(&rhs.body.select, OneSelect::Select { from: None, .. }) {
                    if let Err(error) = qualify_correlated_using(
                        connection,
                        params,
                        ctes,
                        rhs,
                        sources,
                        &inherited_using,
                    ) {
                        nested_error = Some(error);
                    }
                }
                return Ok(turso_core::WalkControl::Continue);
            }

            // Source-free wrappers around sourceful scalars already use the
            // logical correlation pass, which preserves encoded outer values.
            let scalar = matches!(expr, Expr::Subquery(inner) if sourceful || matches!(&inner.body.select, OneSelect::Select { from: None, .. }));
            if matches!(expr, Expr::Exists(_)) || scalar {
                let (Expr::Exists(inner) | Expr::Subquery(inner)) = expr else {
                    unreachable!()
                };
                if let Err(error) = qualify_correlated_using(
                    connection,
                    params,
                    ctes,
                    inner,
                    sources,
                    &inherited_using,
                ) {
                    nested_error = Some(error);
                }
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            if matches!(
                expr,
                Expr::Subquery(_) | Expr::Exists(_) | Expr::InSelect { .. }
            ) {
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            if let Expr::Id(name) | Expr::Name(name) = expr {
                if preserves_aliases && aliases.contains(&name.as_str().to_ascii_lowercase()) {
                    return Ok(turso_core::WalkControl::SkipChildren);
                }
                if let Some((index, column)) =
                    using.bindings.get(&name.as_str().to_ascii_lowercase())
                {
                    *expr = Expr::Qualified(
                        Name::exact(sources[*index].alias.clone()),
                        Name::exact(column.clone()),
                    );
                }
            }
            Ok(turso_core::WalkControl::Continue)
        })?;
        if let Some(error) = nested_error {
            return Err(error);
        }
    }
    Ok(())
}
fn prepare_using(from: Option<&mut FromClause>, sources: &[Source]) -> Result<UsingColumns> {
    let mut merged = UsingColumns::default();
    let Some(from) = from else { return Ok(merged) };
    if !from
        .joins
        .iter()
        .any(|join| matches!(join.constraint, Some(JoinConstraint::Using(_)))
            || matches!(join.operator, JoinOperator::TypedJoin(Some(kind)) if kind.contains(JoinType::NATURAL)))
    {
        return Ok(merged);
    }
    let column = |index: usize, name: &str| -> Option<String> {
        let source = &sources[index];
        if source.collection.is_some() {
            return Some(name.into());
        }
        source.derived.as_ref().and_then(|columns| {
            columns
                .iter()
                .position(|(column, _)| column.eq_ignore_ascii_case(name))
                .map(|position| {
                    source.derived_physical.as_ref().map_or_else(
                        || columns[position].0.clone(),
                        |names| names[position].clone(),
                    )
                })
        })
    };
    let mut order = vec![0];
    for (position, join) in from.joins.iter_mut().enumerate() {
        order.push(position + 1);
        let kind = match join.operator {
            JoinOperator::TypedJoin(Some(kind)) => Some(kind),
            _ => None,
        };
        let right = kind
            .is_some_and(|kind| kind.contains(JoinType::RIGHT) && !kind.contains(JoinType::LEFT));
        if right {
            if position != 0 {
                return Err(unsupported("RIGHT JOIN following another join"));
            }
            order.swap(0, 1);
        }
        if kind.is_some_and(|kind| kind.contains(JoinType::NATURAL)) {
            if join.constraint.is_some() {
                return Err(Error::Validation(
                    "NATURAL JOIN cannot have ON or USING".into(),
                ));
            }
            if order.iter().any(|index| {
                sources[*index].collection.is_some() || sources[*index].derived.is_none()
            }) {
                return Err(unsupported(
                    "NATURAL JOIN requires closed sources; project collection fields explicitly",
                ));
            }
            let right_index = *order.last().unwrap();
            let mut seen = std::collections::BTreeSet::new();
            let names = sources[right_index]
                .derived
                .as_ref()
                .unwrap()
                .iter()
                .filter_map(|(name, _)| {
                    let key = name.to_ascii_lowercase();
                    (order[..order.len() - 1]
                        .iter()
                        .any(|index| column(*index, name).is_some())
                        && seen.insert(key))
                    .then(|| Name::exact(name.clone()))
                })
                .collect();
            join.constraint = Some(JoinConstraint::Using(names));
            join.operator = JoinOperator::TypedJoin(kind.map(|kind| kind & !JoinType::NATURAL));
        }
        let Some(JoinConstraint::Using(names)) = &join.constraint else {
            continue;
        };
        if kind.is_some_and(|kind| {
            (kind.contains(JoinType::LEFT) && kind.contains(JoinType::RIGHT))
                || (kind.contains(JoinType::OUTER)
                    && !kind.contains(JoinType::LEFT)
                    && !kind.contains(JoinType::RIGHT))
        }) {
            return Err(unsupported("FULL JOIN USING"));
        }
        let right_index = *order.last().unwrap();
        let mut predicate = None;
        for name in names {
            let key = name.as_str().to_ascii_lowercase();
            let left = order[..order.len() - 1]
                .iter()
                .find_map(|index| column(*index, name.as_str()).map(|name| (*index, name)))
                .ok_or_else(|| {
                    Error::Validation(format!(
                        "USING column {} is absent from the left source",
                        name.as_str()
                    ))
                })?;
            let right_column = column(right_index, name.as_str()).ok_or_else(|| {
                Error::Validation(format!(
                    "USING column {} is absent from the right source",
                    name.as_str()
                ))
            })?;
            let equality = expression(&format!(
                "{}.{} = {}.{}",
                quote(&sources[left.0].alias),
                quote(&left.1),
                quote(&sources[right_index].alias),
                quote(&right_column)
            ))?;
            predicate = Some(match predicate {
                None => equality,
                Some(previous) => {
                    Expr::Binary(Box::new(previous), Operator::And, Box::new(equality))
                }
            });
            merged.bindings.insert(key.clone(), left);
            merged.hidden.insert((right_index, key));
        }
        join.constraint = Some(JoinConstraint::On(Box::new(
            predicate.unwrap_or(expression("1")?),
        )));
    }
    Ok(merged)
}
struct Scope {
    using: UsingColumns,
    qualified_only: bool,
    expression_subqueries: ExpressionSubqueries,
    sources: Vec<Source>,
    params: Parameters,
    consumed: std::cell::RefCell<std::collections::BTreeSet<String>>,
    fetched_aliases: std::cell::RefCell<std::collections::BTreeSet<String>>,
    standalone_aliases: std::cell::RefCell<std::collections::BTreeMap<String, (Expr, bool)>>,
}
impl Scope {
    fn qualify_using(&self, expr: &mut Expr) {
        if self.using.bindings.is_empty() {
            return;
        }
        if let Expr::Id(name) | Expr::Name(name) = expr {
            let key = name.as_str().to_ascii_lowercase();
            if !self.standalone_aliases.borrow().contains_key(&key) {
                if let Some((index, column)) = self.using.bindings.get(&key) {
                    *expr = Expr::Qualified(
                        Name::exact(self.sources[*index].alias.clone()),
                        Name::exact(column.clone()),
                    );
                }
            }
        }
    }
    fn field(&self, expr: &Expr) -> Result<Option<(usize, Vec<String>)>> {
        let mut qualified;
        let expr = if matches!(expr, Expr::Id(_) | Expr::Name(_)) && !self.using.bindings.is_empty()
        {
            qualified = expr.clone();
            self.qualify_using(&mut qualified);
            &qualified
        } else {
            expr
        };
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
            if self.qualified_only {
                return Ok(None);
            }
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
                // Derived projections have a closed column set, so an unqualified
                // column can be resolved without guessing about document fields.
                if self.sources.iter().all(|source| source.derived.is_some()) {
                    let mut matches = self.sources.iter().enumerate().filter(|(_, source)| {
                        source
                            .derived
                            .as_ref()
                            .unwrap()
                            .iter()
                            .any(|(name, _)| name.eq_ignore_ascii_case(&parts[0]))
                    });
                    let first = matches.next();
                    if matches.next().is_none() {
                        return Ok(first.and_then(|(i, source)| {
                            source.typed_field(&parts).then_some((i, parts))
                        }));
                    }
                }
                return Err(Error::Validation(
                    "qualify fields in collection joins".into(),
                ));
            }
            return Ok(self.sources[0].typed_field(&parts).then_some((0, parts)));
        }
        let Some(i) = self
            .sources
            .iter()
            .position(|s| s.alias.eq_ignore_ascii_case(&parts[0]))
        else {
            return Ok(None);
        };
        Ok(self.sources[i]
            .typed_field(&parts[1..])
            .then(|| (i, parts[1..].to_vec())))
    }
    fn accessor(&self, i: usize, path: &[String], typed: bool) -> Result<Expr> {
        if self.sources[i].derived.is_some() {
            let Some((column, nested)) = path.split_first() else {
                return Err(unsupported("doc::row on derived sources"));
            };
            let value = format!("{}.{}", quote(&self.sources[i].alias), quote(column));
            if nested.is_empty() {
                return expression(&if typed {
                    value
                } else {
                    format!("__fastdb_unwrap({value})")
                });
            }
            let path = serde_json::to_string(nested)?.replace('\'', "''");
            let value = format!("__fastdb_nested_value({value},'{path}')");
            return expression(&if typed {
                value
            } else {
                format!("__fastdb_unwrap({value})")
            });
        }
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
    fn native_scalar_query(&self, expr: &Expr) -> Expr {
        let (lowered, _, _) = &self.expression_subqueries[&order_base(expr).to_string()];
        let Expr::FunctionCall { args, .. } = lowered else {
            unreachable!("packed native scalar")
        };
        let mut runtime = expr.clone();
        replace_order_base(&mut runtime, *args[0].clone());
        runtime
    }
    fn preserved(&self, expr: &mut Expr) -> Result<bool> {
        self.qualify_using(expr);
        // This compiler-only marker carries a derived column's logical type
        // across recursive lowering without adding a runtime conversion.
        if let Expr::FunctionCall { name, args, .. } = expr {
            if name.as_str() == "__fastdb_correlated_value" && args.len() == 1 {
                *expr = *args[0].clone();
                return Ok(true);
            }
        }
        // Accessors inserted for an outer collection already return encoded
        // logical values. Packing them again would turn records into binary.
        if matches!(expr, Expr::FunctionCall { name, .. } if name.as_str() == "__fastdb_value") {
            return Ok(true);
        }
        if matches!(expr, Expr::Subquery(_)) {
            if let Some((lowered, consumed, _)) = self.expression_subqueries.get(&expr.to_string())
            {
                for name in consumed {
                    if !self.params.contains_key(name) {
                        return Err(Error::Parameter(name.clone()));
                    }
                }
                self.consumed.borrow_mut().extend(consumed.iter().cloned());
                *expr = lowered.clone();
                return Ok(true);
            }
        }
        if let Some((value, typed)) = self.standalone_alias(expr) {
            if typed {
                *expr = value;
            }
            return Ok(typed);
        }
        if blob_literal(expr) {
            *expr = expression(&format!("__fastdb_pack({})", order_base(expr)))?;
            return Ok(true);
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
    fn native_column(&self, expr: &Expr) -> Result<bool> {
        match expr {
            Expr::Collate(value, _) | Expr::Unary(UnaryOperator::Positive, value) => {
                self.native_column(value)
            }
            Expr::Parenthesized(values) if values.len() == 1 => self.native_column(&values[0]),
            _ if native_column_reference(expr) => Ok(!self.preserved(&mut expr.clone())?),
            _ => Ok(false),
        }
    }
    fn derived_native_collation<'a>(
        &'a self,
        expr: &Expr,
        expression_collation: bool,
    ) -> Option<String> {
        let collations = |source: &'a Source| {
            if expression_collation {
                &source.native_expression_collations
            } else {
                &source.native_collations
            }
        };
        match expr {
            Expr::Collate(_, name) => Some(name.as_str().to_owned()),
            Expr::Unary(UnaryOperator::Positive, value) => {
                self.derived_native_collation(value, expression_collation)
            }
            Expr::Parenthesized(values) if values.len() == 1 => {
                self.derived_native_collation(&values[0], expression_collation)
            }
            Expr::Qualified(alias, column) => collations(
                self.sources
                    .iter()
                    .find(|source| source.alias.eq_ignore_ascii_case(alias.as_str()))?,
            )
            .get(&column.as_str().to_ascii_lowercase())
            .cloned(),
            Expr::Id(column) | Expr::Name(column) => {
                let mut found = self.sources.iter().filter_map(|source| {
                    collations(source).get(&column.as_str().to_ascii_lowercase())
                });
                let first = found.next()?;
                found.next().is_none().then(|| first.clone())
            }
            _ => None,
        }
    }
    fn sql_argument(&self, expr: &mut Expr) -> Result<()> {
        if self.preserved(expr)? {
            *expr = expression(&format!("__fastdb_sql_scalar({expr})"))?;
            Ok(())
        } else {
            self.lower(expr)
        }
    }
    fn comparison_key(&self, expr: &mut Expr) -> Result<bool> {
        // Keep explicit collation and unary plus outside the conversion.
        // Plus preserves the scalar value while removing SQL affinity.
        match expr {
            Expr::Collate(value, _) | Expr::Unary(UnaryOperator::Positive, value) => {
                return self.comparison_key(value);
            }
            Expr::Parenthesized(values) if values.len() == 1 => {
                return self.comparison_key(&mut values[0]);
            }
            _ => {}
        }
        if let Some((mut value, typed)) = self.standalone_alias(expr) {
            if typed {
                *expr = expression(&format!("__fastdb_unwrap({value})"))?;
                return Ok(true);
            }
            if native_alias_key(&mut value)? {
                *expr = value;
                return Ok(true);
            }
            return Ok(false);
        }
        if let Some((i, path)) = self.field(expr)? {
            // Direct IDs must remain column references for primary-key seeks.
            *expr = self.accessor(i, &path, false)?;
            return Ok(true);
        }
        if self.preserved(expr)? {
            *expr = expression(&format!("__fastdb_unwrap({expr})"))?;
            return Ok(true);
        }
        let cast_type = match expr {
            Expr::Cast { type_name, .. } => Some(type_name.clone()),
            _ => None,
        };
        if matches!(
            expr,
            Expr::FunctionCall { .. }
                | Expr::FunctionCallStar { .. }
                | Expr::Cast { .. }
                | Expr::Literal(_)
                | Expr::Unary(_, _)
                | Expr::Binary(_, _, _)
                | Expr::Between { .. }
                | Expr::InList { .. }
                | Expr::IsNull(_)
                | Expr::NotNull(_)
                | Expr::Like { .. }
        ) {
            self.typed(expr)?;
            *expr = expression(&format!("__fastdb_unwrap({expr})"))?;
            if let Some(type_name) = cast_type {
                // The conversion returns the already-cast scalar (or its binary
                // key). Repeating CAST preserves its affinity for IN coercion.
                *expr = Expr::Cast {
                    expr: Box::new(expr.clone()),
                    type_name,
                };
            }
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
        // Correlation accessors already return encoded values. A collation
        // wrapper must not cause those values to be packed a second time.
        if let Expr::Collate(value, _) = expr {
            if matches!(order_base(value), Expr::FunctionCall { name, .. }
                if matches!(name.as_str(), "__fastdb_value" | "__fastdb_correlated_value"))
            {
                return self.preserved(value);
            }
        }
        if let Expr::Unary(UnaryOperator::Positive, value) = expr {
            return self.preserved(value);
        }
        if let Expr::Case {
            base,
            when_then_pairs,
            else_expr,
        } = expr
        {
            let keyed = if let Some(base) = base {
                if self.comparison_key(base)? {
                    true
                } else {
                    self.lower(base)?;
                    false
                }
            } else {
                false
            };
            for (condition, value) in when_then_pairs {
                if base.is_none() {
                    self.sql_argument(condition)?;
                } else if keyed {
                    if !self.comparison_key(condition)? {
                        self.typed(condition)?;
                        **condition = expression(&format!("__fastdb_unwrap({condition})"))?;
                    }
                } else {
                    self.lower(condition)?;
                }
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
                    *arg = Box::new(vector_input_expression(arg)?);
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
        self.qualify_using(expr);
        if matches!(expr, Expr::InSelect { .. }) {
            if let Some((query, consumed, column)) =
                self.expression_subqueries.get(&expr.to_string())
            {
                let Expr::InSelect { lhs, not, rhs } = expr else {
                    unreachable!()
                };
                for name in consumed {
                    if !self.params.contains_key(name) {
                        return Err(Error::Parameter(name.clone()));
                    }
                }
                self.consumed.borrow_mut().extend(consumed.iter().cloned());
                if let SubqueryAffinity::NativeMembership(collation, shared) = column {
                    let direct_keys = shared.is_empty()
                        && self.sources.iter().any(Source::logical)
                        && matches!(query, Expr::Subquery(inner) if !inner.body.compounds.is_empty() && inner.order_by.is_empty() && direct_compound_pagination(inner));
                    let mut value = *lhs.clone();
                    let is_column = membership_column(&value)
                        || matches!(order_base(&value), Expr::Cast { .. });
                    if !self.comparison_key(&mut value)? {
                        self.lower(&mut value)?;
                        if direct_keys {
                            value =
                                expression(&format!("__fastdb_unwrap(__fastdb_pack({value}))"))?;
                        } else {
                            **lhs = value;
                            if shared.is_empty() {
                                let Expr::Subquery(inner) = query else {
                                    unreachable!()
                                };
                                *rhs = inner.clone();
                            }
                            return Ok(());
                        }
                    }
                    let key = match order_base(lhs) {
                        Expr::Cast { type_name, .. } => Expr::Cast {
                            expr: Box::new(expression("k")?),
                            type_name: type_name.clone(),
                        }
                        .to_string(),
                        _ => {
                            if is_column {
                                "k".to_owned()
                            } else {
                                "+k".to_owned()
                            }
                        }
                    };
                    let mut native_value = "v".to_owned();
                    if let Expr::Subquery(inner) = query {
                        if let OneSelect::Select { columns, .. } = &inner.body.select {
                            if let [ResultColumn::Expr(value, _)] = columns.as_slice() {
                                native_value = match order_base(value) {
                                    Expr::Cast { type_name, .. } => Expr::Cast {
                                        expr: Box::new(expression("v")?),
                                        type_name: type_name.clone(),
                                    }
                                    .to_string(),
                                    _ if membership_column(value) => "v".to_owned(),
                                    _ => "+v".to_owned(),
                                };
                            }
                        }
                    }
                    let collation = outer_collation(lhs).unwrap_or(collation);
                    let key = format!("({key} COLLATE {})", quote(collation));
                    let negate = if *not { "NOT " } else { "" };
                    // Keep native IN execution so its uncorrelated source can
                    // be cached across outer rows. Only BLOB comparison keys
                    // require encoding of the source values.
                    if direct_keys {
                        // Both sides use comparison keys; retain one native RHS.
                        *expr = expression(&format!("(WITH __fastdb_member_lhs(k) AS NOT MATERIALIZED (SELECT {value}) SELECT {key} {negate}IN {query} FROM __fastdb_member_lhs)"))?;
                        return Ok(());
                    }
                    let local = if shared.is_empty() {
                        format!(", __fastdb_correlated_members(v) AS MATERIALIZED {query}")
                    } else {
                        String::new()
                    };
                    let shared = if shared.is_empty() {
                        "__fastdb_correlated_members"
                    } else {
                        shared.as_str()
                    };
                    *expr = expression(&format!("(WITH __fastdb_member_lhs(k) AS NOT MATERIALIZED (SELECT {value}){local} SELECT CASE WHEN typeof(k)='blob' THEN {key} {negate}IN (SELECT __fastdb_unwrap(__fastdb_pack(v)) FROM {shared}) ELSE {key} {negate}IN (SELECT {native_value} FROM {shared}) END FROM __fastdb_member_lhs)"))?;
                    return Ok(());
                }
                let mut value = *lhs.clone();
                let negate = if *not { "NOT " } else { "" };
                if self.comparison_key(&mut value)? {
                    *expr = expression(&format!("{value} {negate}IN {query}"))?;
                } else if native_column_reference(&value) {
                    // The generated scalar wrapper introduces a new scope. Keep
                    // a resolved native derived column attached to its source.
                    if let Expr::Id(name) | Expr::Name(name) = &value {
                        let mut matches = self.sources.iter().filter_map(|source| {
                            let columns = source.derived.as_ref()?;
                            let position = columns.iter().position(|(column, _)| {
                                column.eq_ignore_ascii_case(name.as_str())
                            })?;
                            Some((source, position))
                        });
                        if let Some((source, position)) = matches.next() {
                            if matches.next().is_none() {
                                let column = source.derived_physical.as_ref().map_or_else(
                                    || source.derived.as_ref().unwrap()[position].0.clone(),
                                    |columns| columns[position].clone(),
                                );
                                value = Expr::Qualified(
                                    Name::exact(source.alias.clone()),
                                    Name::exact(column),
                                );
                            }
                        }
                    }
                    // Preserve native LHS affinity for scalar values; encode its
                    // BLOB values into the same collision-resistant RHS keys. Share
                    // one materialized source across both IN branches.
                    let key = if *column == SubqueryAffinity::MembershipColumn {
                        "v"
                    } else {
                        "+v"
                    };
                    *expr = expression(&format!("(WITH __fastdb_in_source(v) AS MATERIALIZED {query} SELECT CASE WHEN typeof({value})='blob' THEN __fastdb_unwrap(__fastdb_pack({value})) {negate}IN (SELECT {key} FROM __fastdb_in_source) ELSE {value} {negate}IN (SELECT {key} FROM __fastdb_in_source) END)"))?;
                } else {
                    self.typed(&mut value)?;
                    *expr = expression(&format!("__fastdb_unwrap({value}) {negate}IN {query}"))?;
                }
                return Ok(());
            }
        }
        if matches!(expr, Expr::Exists(_)) {
            if let Some((lowered, consumed, _)) = self.expression_subqueries.get(&expr.to_string())
            {
                for name in consumed {
                    if !self.params.contains_key(name) {
                        return Err(Error::Parameter(name.clone()));
                    }
                }
                self.consumed.borrow_mut().extend(consumed.iter().cloned());
                *expr = lowered.clone();
                return Ok(());
            }
        }
        if blob_literal(expr) {
            return Ok(());
        }
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
                    *arg = Box::new(vector_input_expression(arg)?);
                }
                return Ok(());
            }
        }
        if matches!(expr, Expr::FunctionCall {name,..} if name.as_str()=="__fastdb_fetch") {
            return Err(unsupported(
                "record::fetch is allowed only as a top-level SELECT projection",
            ));
        }
        if (matches!(expr, Expr::Subquery(_))
            || matches!(expr, Expr::FunctionCall { name, .. } if name.as_str() == "__fastdb_correlated_value"))
            && self.preserved(expr)?
        {
            *expr = expression(&format!("__fastdb_unwrap({expr})"))?;
            return Ok(());
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
                    Operator::Equals
                        | Operator::NotEquals
                        | Operator::Is
                        | Operator::IsNot
                        | Operator::Less
                        | Operator::LessEquals
                        | Operator::Greater
                        | Operator::GreaterEquals
                ) {
                    let native = |value: &Expr| {
                        let value = order_base(value);
                        matches!(value, Expr::Subquery(_))
                            && self
                                .expression_subqueries
                                .get(&value.to_string())
                                .is_some_and(|(_, _, affinity)| {
                                    matches!(affinity, SubqueryAffinity::NativeScalar(_))
                                })
                    };
                    let (left_native, right_native) = (native(a), native(b));
                    if left_native || right_native {
                        let (query, value, on_left) = if left_native {
                            (&**a, &**b, true)
                        } else {
                            (&**b, &**a, false)
                        };
                        // Validate bindings while retaining the original SQL affinity.
                        self.preserved(&mut order_base(query).clone())?;
                        let mut logical = order_base(value).clone();
                        if native(value) {
                            self.preserved(&mut logical)?;
                            **a = self.native_scalar_query(a);
                            **b = self.native_scalar_query(b);
                            return Ok(());
                        }
                        let native_parameter = if let Expr::Variable(var) = &logical {
                            let name = var
                                .name
                                .as_ref()
                                .map_or_else(|| format!("?{}", var.index), |name| name.to_string());
                            self.params.get(&name).is_some_and(|value| {
                                matches!(
                                    value,
                                    Value::Null
                                        | Value::Integer(_)
                                        | Value::Number(_)
                                        | Value::String(_)
                                )
                            })
                        } else {
                            false
                        };
                        // Ordinary scalar parameters must retain the native
                        // comparison boundary so a scalar CAST applies affinity.
                        if native_parameter || !self.preserved(&mut logical)? {
                            let mut lowered = value.clone();
                            self.lower(&mut lowered)?;
                            if on_left {
                                **a = self.native_scalar_query(query);
                                **b = lowered;
                            } else {
                                **b = self.native_scalar_query(query);
                                **a = lowered;
                            }
                            return Ok(());
                        }

                        let runtime = self.native_scalar_query(query);
                        let Expr::Subquery(inner) = order_base(&runtime) else {
                            unreachable!()
                        };
                        let sql = Cmd::Stmt(Stmt::Select(inner.clone())).to_string();
                        let SubqueryAffinity::NativeScalar(collation) =
                            &self.expression_subqueries[&order_base(query).to_string()].2
                        else {
                            unreachable!()
                        };
                        let native_explicit = outer_collation(query);
                        let logical_explicit = outer_collation(value);
                        let explicit = if on_left {
                            native_explicit.or(logical_explicit)
                        } else {
                            logical_explicit.or(native_explicit)
                        };
                        let collation =
                            explicit.unwrap_or(if on_left { collation } else { "BINARY" });
                        let raw = format!(
                            "((SELECT * FROM __fastdb_native_comparison) COLLATE {})",
                            quote(collation)
                        );
                        let raw = raw.as_str();
                        let key =
                            format!("(SELECT v FROM (SELECT __fastdb_unwrap({logical}) AS v))");
                        let compare = |l: &str, r: &str| {
                            if on_left {
                                format!("{r} {op} {l}")
                            } else {
                                format!("{l} {op} {r}")
                            }
                        };
                        let body = if matches!(
                            op,
                            Operator::Equals | Operator::NotEquals | Operator::Is | Operator::IsNot
                        ) {
                            let blob = format!("__fastdb_unwrap(__fastdb_pack({raw}))");
                            format!(
                                "CASE WHEN typeof({raw})='blob' THEN {} ELSE {} END",
                                compare(&key, &blob),
                                compare(&key, raw)
                            )
                        } else {
                            let value = format!("(SELECT v FROM (SELECT __fastdb_range_scalar({logical},{raw}) AS v))");
                            // Keep the document operand's default collation when
                            // it precedes a native scalar with declared collation.
                            compare(
                                &if on_left {
                                    value
                                } else {
                                    format!("({value} COLLATE {})", quote(collation))
                                },
                                raw,
                            )
                        };
                        *expr = expression(&format!(
                            "(WITH __fastdb_native_comparison AS MATERIALIZED (SELECT * FROM ({}) LIMIT 1) SELECT {body})", sql.trim().trim_end_matches(';')
                        ))?;
                        return Ok(());
                    }
                }
                if matches!(
                    op,
                    Operator::Add
                        | Operator::Subtract
                        | Operator::Multiply
                        | Operator::Divide
                        | Operator::Modulus
                        | Operator::Concat
                        | Operator::ArrowRight
                        | Operator::ArrowRightShift
                        | Operator::BitwiseAnd
                        | Operator::BitwiseOr
                        | Operator::LeftShift
                        | Operator::RightShift
                        | Operator::And
                        | Operator::Or
                ) {
                    self.sql_argument(a)?;
                    self.sql_argument(b)?;
                    return Ok(());
                }
                if matches!(
                    op,
                    Operator::Equals | Operator::NotEquals | Operator::Is | Operator::IsNot
                ) {
                    let (mut left, mut right) = (*a.clone(), *b.clone());
                    let left_key = self.comparison_key(&mut left)?;
                    let right_key = self.comparison_key(&mut right)?;
                    if left_key && right_key {
                        **a = left;
                        **b = right;
                        return Ok(());
                    }
                    let native = if !left_key && right_key && native_column_reference(&left) {
                        Some((&left, true))
                    } else if left_key && !right_key && native_column_reference(&right) {
                        Some((&right, false))
                    } else {
                        None
                    };
                    if let Some((column, on_left)) = native {
                        // Only BLOB values need a comparison key. Keep the raw
                        // column in the other branch so native affinity and
                        // implicit collation survive expression lowering.
                        let key = format!("__fastdb_unwrap(__fastdb_pack({column}))");
                        let binary = if on_left {
                            format!("{key} {op} {right}")
                        } else {
                            format!("{left} {op} {key}")
                        };
                        // Accessor arguments refer to physical BLOB columns.
                        // Visit the native column first so their incidental
                        // collation cannot hide its declared collation. Keep
                        // explicit COLLATE precedence in the original order.
                        let expression_collation = (!on_left
                            && (matches!(op, Operator::Is | Operator::IsNot)
                                || !membership_column(column))
                            && !native_column_collation(&left))
                        .then(|| self.derived_native_collation(column, true))
                        .flatten();
                        let scalar = if let Some(collation) = expression_collation {
                            format!(
                                "({left} COLLATE {}) {op} ({right} COLLATE {})",
                                quote(&collation),
                                quote(&collation)
                            )
                        } else if !on_left && !native_column_collation(column) {
                            format!("{right} {op} {left}")
                        } else {
                            format!("{left} {op} {right}")
                        };
                        *expr = expression(&format!(
                            "CASE WHEN typeof({column})='blob' THEN {binary} ELSE {scalar} END"
                        ))?;
                        return Ok(());
                    }
                }
                if matches!(
                    op,
                    Operator::Less
                        | Operator::LessEquals
                        | Operator::Greater
                        | Operator::GreaterEquals
                ) {
                    let (mut left, mut right) = (*a.clone(), *b.clone());
                    let left_typed = self.preserved(&mut left)?;
                    let right_typed = self.preserved(&mut right)?;
                    if left_typed && right_typed {
                        if native_column_collation(a) || native_column_collation(b) {
                            let (mut left, mut right) = (*a.clone(), *b.clone());
                            if self.comparison_key(&mut left)? && self.comparison_key(&mut right)? {
                                **a = left;
                                **b = right;
                                return Ok(());
                            }
                        }
                        *expr = expression(&format!("__fastdb_compare({left}, {right}) {op} 0"))?;
                        return Ok(());
                    }
                    if left_typed && !right_typed && self.native_column(&right)? {
                        left = expression(&format!("__fastdb_range_scalar({left}, {right})"))?;
                        // As for equality, prioritize the native column's
                        // declared collation over physical document storage.
                        // Reversing operands also reverses the range operator.
                        *expr = if native_column_collation(&right) {
                            expression(&format!("{left} {op} {right}"))?
                        } else {
                            let reverse = match op {
                                Operator::Less => Operator::Greater,
                                Operator::LessEquals => Operator::GreaterEquals,
                                Operator::Greater => Operator::Less,
                                Operator::GreaterEquals => Operator::LessEquals,
                                _ => unreachable!("range operator checked above"),
                            };
                            expression(&format!("{right} {reverse} {left}"))?
                        };
                        return Ok(());
                    }
                    if right_typed && !left_typed && self.native_column(&left)? {
                        *expr = expression(&format!(
                            "{left} {op} __fastdb_range_scalar({right}, {left})"
                        ))?;
                        return Ok(());
                    }
                }
                self.lower(a)?;
                self.lower(b)?;
            }
            Expr::Unary(UnaryOperator::Positive, e)
            | Expr::IsNull(e)
            | Expr::NotNull(e)
            | Expr::Collate(e, _) => self.lower(e)?,
            Expr::Unary(_, e) => self.sql_argument(e)?,
            Expr::Cast { expr: e, .. } => self.sql_argument(e)?,
            Expr::Between {
                lhs,
                start,
                end,
                not,
            } => {
                let (mut value, mut lower, mut upper) =
                    (*lhs.clone(), *start.clone(), *end.clone());
                let value_typed = self.preserved(&mut value)?;
                let lower_typed = self.preserved(&mut lower)?;
                let upper_typed = self.preserved(&mut upper)?;
                if value_typed && lower_typed && upper_typed {
                    if native_column_collation(lhs)
                        || native_column_collation(start)
                        || native_column_collation(end)
                    {
                        let (mut value, mut lower, mut upper) =
                            (*lhs.clone(), *start.clone(), *end.clone());
                        if self.comparison_key(&mut value)?
                            && self.comparison_key(&mut lower)?
                            && self.comparison_key(&mut upper)?
                        {
                            // Retain BETWEEN for one lhs evaluation and each
                            // bound's independent native collation precedence.
                            **lhs = value;
                            **start = lower;
                            **end = upper;
                            return Ok(());
                        }
                    }
                    let negate = if *not { "NOT " } else { "" };
                    *expr = expression(&format!(
                        "{negate}__fastdb_between({value}, {lower}, {upper})"
                    ))?;
                    return Ok(());
                }
                if value_typed
                    && !lower_typed
                    && !upper_typed
                    && self.native_column(&lower)?
                    && self.native_column(&upper)?
                {
                    // Retain BETWEEN so the engine evaluates the logical lhs
                    // once and applies each native bound's affinity. A record
                    // cannot be ordered against either non-null SQL bound.
                    // Isolate the conversion in a scalar subquery so the
                    // engine does not propagate physical document collation
                    // (or the other bound's collation) into either comparison.
                    **lhs = expression(&format!(
                        "(SELECT __fastdb_range_scalar({value}, coalesce({lower}, {upper})))"
                    ))?;
                    return Ok(());
                }
                if !value_typed && (lower_typed || upper_typed) && self.native_column(&value)? {
                    // Keep the native BETWEEN node: the engine applies the
                    // column's affinity independently to each bound. Typed
                    // bounds are converted once, retaining raw binary bytes.
                    for (bound, lowered, typed) in
                        [(start, lower, lower_typed), (end, upper, upper_typed)]
                    {
                        if typed {
                            **bound =
                                expression(&format!("__fastdb_range_scalar({lowered}, {value})"))?;
                        } else {
                            self.lower(bound)?;
                        }
                    }
                    return Ok(());
                }
                self.lower(lhs)?;
                self.lower(start)?;
                self.lower(end)?;
            }
            Expr::Like {
                lhs, rhs, escape, ..
            } => {
                self.sql_argument(lhs)?;
                self.sql_argument(rhs)?;
                if let Some(e) = escape {
                    self.sql_argument(e)?;
                }
            }
            Expr::InList { lhs, rhs, not } => {
                let mut value = *lhs.clone();
                if self.comparison_key(&mut value)? {
                    // Use one collision-resistant scalar representation for the
                    // whole list, including native functions returning blobs.
                    **lhs = value;
                    for value in rhs.iter_mut() {
                        if !self.comparison_key(value)? {
                            self.typed(value)?;
                            **value = expression(&format!("__fastdb_unwrap({value})"))?;
                        }
                    }
                    return Ok(());
                }
                if native_column_reference(&value) {
                    for value in rhs.iter_mut() {
                        if !self.comparison_key(value)? {
                            self.typed(value)?;
                            **value = expression(&format!("__fastdb_unwrap({value})"))?;
                        }
                    }
                    if let Some(collation) = self.derived_native_collation(&value, false) {
                        value = expression(&format!("({value} COLLATE {})", quote(&collation)))?;
                        for member in rhs.iter_mut() {
                            **member =
                                expression(&format!("({member} COLLATE {})", quote(&collation)))?;
                        }
                    }
                    let list = rhs
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",");
                    let negate = if *not { "NOT " } else { "" };
                    // IN ignores RHS affinity. Preserve the native LHS column's
                    // affinity for scalar values and encode only its BLOB values.
                    *expr = expression(&format!(
                        "CASE WHEN typeof({value})='blob' THEN __fastdb_unwrap(__fastdb_pack({value})) {negate}IN ({list}) ELSE {value} {negate}IN ({list}) END"
                    ))?;
                    return Ok(());
                }
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
                    if base.is_none() {
                        self.sql_argument(a)?;
                    } else {
                        self.lower(a)?;
                    }
                    self.lower(b)?;
                }
                if let Some(e) = else_expr {
                    self.lower(e)?;
                }
            }
            Expr::FunctionCall {
                name,
                args,
                order_by,
                within_group,
                filter_over,
                distinctness,
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
                    if name.as_str().eq_ignore_ascii_case("count")
                        && !matches!(distinctness, Some(Distinctness::Distinct))
                    {
                        self.typed(e)?;
                        **e = expression(&format!("__fastdb_count_value({e})"))?;
                    } else if name.as_str().starts_with("__fastdb_") {
                        self.lower(e)?;
                    } else {
                        self.sql_argument(e)?;
                    }
                }
                for s in order_by.iter_mut().chain(within_group) {
                    self.lower(&mut s.expr)?;
                }
                if let Some(e) = &mut filter_over.filter_clause {
                    self.sql_argument(e)?;
                }
            }
            Expr::FunctionCallStar { filter_over, .. } => {
                if let Some(Over::Window(window)) = &mut filter_over.over_clause {
                    self.lower_window(window)?;
                }
                if let Some(e) = &mut filter_over.filter_clause {
                    self.sql_argument(e)?;
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
    crate::parser_stack(|| parsed_inner(sql))
}
fn parsed_inner(sql: &str) -> Result<Cmd> {
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
// Keep generated relation names distinct from every explicit name in this scope.
fn anonymous_source_alias(from: &FromClause, position: usize) -> String {
    let mut candidate = format!("__fastdb_anonymous_{position}");
    loop {
        let occupied = std::iter::once(&from.select)
            .chain(from.joins.iter().map(|j| &j.table))
            .any(|table| {
                let name = match table.as_ref() {
                    SelectTable::Table(name, alias, _) => Some(
                        alias
                            .as_ref()
                            .map_or(name.name.as_str(), |a| a.name().as_str()),
                    ),
                    SelectTable::Select(_, alias) => alias.as_ref().map(|a| a.name().as_str()),
                    _ => None,
                };
                name.is_some_and(|name| name.eq_ignore_ascii_case(&candidate))
            });
        if !occupied {
            return candidate;
        }
        candidate.push('_');
    }
}
fn derived_physical_names(names: &[String]) -> Vec<String> {
    // Build once: scanning all source names for every duplicate is quadratic.
    let public_names = names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    let mut seen = std::collections::BTreeSet::new();
    names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if seen.insert(name.to_ascii_lowercase()) {
                name.clone()
            } else {
                let mut private = format!("__fastdb_derived_column_{i}");
                while public_names.contains(&private.to_ascii_lowercase())
                    || !seen.insert(private.to_ascii_lowercase())
                {
                    private.push('_');
                }
                private
            }
        })
        .collect()
}
fn source(
    connection: &Connection,
    table: &SelectTable,
    params: &Parameters,
    ctes: &CteSources,
    native_with: Option<&With>,
    anonymous_alias: String,
    inspect_native: bool,
) -> Result<Source> {
    if let SelectTable::Select(select, alias) = table {
        let generated = As::As(Name::exact(anonymous_alias));
        let alias = alias.as_ref().unwrap_or(&generated);
        let sql = Cmd::Stmt(Stmt::Select(select.clone())).to_string();
        let plan = connection.lower_collection_select(
            &sql,
            &sql,
            params,
            SelectOptions {
                native_with,
                trusted: true,
                nested: true,
                ctes: Some(ctes),
                ..Default::default()
            },
        )?;
        if let Some(plan) = plan {
            if plan.fetched.iter().any(|f| *f) {
                return Err(unsupported("fetched derived projections"));
            }
            let physical = derived_physical_names(&plan.names);
            // Preserve public names and logical types separately from the
            // unique runtime names used to address every projected position.
            let lowered = plan.command.to_string();
            let columns = physical
                .iter()
                .map(|name| quote(name))
                .collect::<Vec<_>>()
                .join(",");
            let Cmd::Stmt(Stmt::Select(select)) = parsed(&format!(
                "WITH __fastdb_derived({columns}) AS ({}) SELECT * FROM __fastdb_derived",
                lowered.trim().trim_end_matches(';')
            ))?
            else {
                unreachable!("derived SELECT wrapper");
            };
            return Ok(Source {
                table: SelectTable::Select(select, Some(alias.clone())),
                alias: alias.name().as_str().into(),
                collection: None,
                derived: Some(plan.names.into_iter().zip(plan.typed).collect()),
                derived_logical: true,
                derived_physical: Some(physical),
                native_collations: Default::default(),
                native_expression_collations: Default::default(),
                consumed: plan.consumed,
            });
        }
        // Ask the engine for native projection names without stepping the query.
        // Retain native types/affinity while exposing the closed column set to
        // mixed derived-source name resolution and star expansion.
        let query = sql.trim().trim_end_matches(';');
        let probe = if let Some(with) = native_with {
            format!("{with} SELECT * FROM ({query})")
        } else {
            format!("SELECT * FROM ({query})")
        };
        let statement = connection.prepare(probe)?;
        let columns: Vec<(String, bool)> = (0..statement.num_columns())
            .map(|i| (statement.get_column_name(i).into_owned(), false))
            .collect();
        let program = statement.get_program();
        // Derived column metadata follows the compound's leftmost output.
        // Expression emission can instead retain the rightmost arm context;
        // keep both so membership and scalar comparisons do not conflate them.
        let mut native_collations = std::collections::BTreeMap::new();
        for (i, column) in program.result_columns.iter().enumerate() {
            let mut value = column.expr.clone();
            let mut implicit = None;
            let mut explicit = None;
            turso_core::walk_expr_mut(&mut value, &mut |expr| {
                match expr {
                    Expr::Collate(_, name) => {
                        explicit.get_or_insert_with(|| name.as_str().to_owned());
                        return Ok(turso_core::WalkControl::SkipChildren);
                    }
                    Expr::Column { table, column, .. } => {
                        if let Some((_, source)) =
                            program.table_references.find_table_by_internal_id(*table)
                        {
                            if let Some(column) = source.get_column_at(*column) {
                                implicit.get_or_insert_with(|| column.collation().name());
                            }
                        }
                    }
                    _ => {}
                }
                Ok(turso_core::WalkControl::Continue)
            })?;
            // Native duplicate-name lookup resolves the first projected column.
            native_collations
                .entry(statement.get_column_name(i).to_ascii_lowercase())
                .or_insert_with(|| explicit.or(implicit).unwrap_or_else(|| "BINARY".into()));
        }
        let mut native_expression_collations = std::collections::BTreeMap::new();
        for (i, column) in program.result_columns.iter().enumerate() {
            let mut pending = vec![(column.expr.clone(), &program.table_references)];
            let mut implicit = None;
            let mut explicit = None;
            while let Some((mut value, tables)) = pending.pop() {
                turso_core::walk_expr_mut(&mut value, &mut |expr| {
                    match expr {
                        Expr::Collate(_, name) => {
                            explicit.get_or_insert_with(|| name.as_str().to_owned());
                            return Ok(turso_core::WalkControl::SkipChildren);
                        }
                        Expr::Column { table, column, .. } => {
                            if let Some((_, source)) = tables.find_table_by_internal_id(*table) {
                                if let turso_core::schema::Table::FromClauseSubquery(derived) =
                                    source
                                {
                                    if let Some(result) =
                                        derived.plan.select_result_columns().get(*column)
                                    {
                                        pending.push((
                                            result.expr.clone(),
                                            derived.plan.select_table_references(),
                                        ));
                                    }
                                } else if let Some(column) = source.get_column_at(*column) {
                                    implicit.get_or_insert_with(|| column.collation().name());
                                }
                            }
                        }
                        _ => {}
                    }
                    Ok(turso_core::WalkControl::Continue)
                })?;
            }
            // Native duplicate-name lookup resolves the first projected column.
            native_expression_collations
                .entry(statement.get_column_name(i).to_ascii_lowercase())
                .or_insert_with(|| explicit.or(implicit).unwrap_or_else(|| "BINARY".into()));
        }
        // A name-based star expansion cannot address later duplicate columns.
        // Preserve the first public name for ordinary lookup and assign private
        // names to subsequent positions through a CTE column list.
        let physical = derived_physical_names(
            &columns
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>(),
        );
        let renamed = physical.iter().zip(&columns).any(|(a, (b, _))| a != b);
        let runtime_select = if renamed {
            let names = physical
                .iter()
                .map(|name| quote(name))
                .collect::<Vec<_>>()
                .join(",");
            let Cmd::Stmt(Stmt::Select(wrapped)) = parsed(&format!(
                "WITH __fastdb_native_derived({names}) AS ({query}) SELECT * FROM __fastdb_native_derived"
            ))? else { unreachable!("native derived SELECT wrapper") };
            wrapped
        } else {
            select.clone()
        };
        return Ok(Source {
            table: SelectTable::Select(runtime_select, Some(alias.clone())),
            alias: alias.name().as_str().into(),
            collection: None,
            derived: Some(columns),
            derived_logical: false,
            derived_physical: renamed.then_some(physical),
            native_collations,
            native_expression_collations,
            consumed: Default::default(),
        });
    }
    let SelectTable::Table(name, alias, indexed) = table else {
        return Err(unsupported("subqueries and table functions"));
    };
    if name.db_name.is_none() {
        if let Some(entry) = ctes.get(&name.name.as_str().to_ascii_lowercase()) {
            let Some(mut source) = entry.clone() else {
                return Err(unsupported("forward or recursive collection CTE reference"));
            };
            if indexed.is_some() {
                return Err(unsupported("INDEXED on a CTE source"));
            }
            source.alias = alias
                .as_ref()
                .map_or(name.name.as_str(), |a| a.name().as_str())
                .into();
            if !source.derived_logical && source.derived_physical.is_some() {
                if let SelectTable::Table(_, runtime_alias, _) = &mut source.table {
                    *runtime_alias = Some(As::As(Name::exact(source.alias.clone())));
                }
            } else {
                source.table = table.clone();
            }
            return Ok(source);
        }
    }
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
    let local_cte = name.db_name.is_none()
        && native_with.is_some_and(|with| {
            with.ctes.iter().any(|cte| {
                cte.tbl_name
                    .as_str()
                    .eq_ignore_ascii_case(name.name.as_str())
            })
        });
    let derived = if inspect_native && collection.is_none() && !local_cte {
        let statement = connection.prepare(format!("SELECT * FROM {table}"))?;
        let program = statement.get_program();
        let columns = (0..statement.num_columns())
            .map(|i| (statement.get_column_name(i).into_owned(), false))
            .collect();
        (!program
            .table_references
            .joined_tables()
            .iter()
            .any(|source| matches!(source.table, turso_core::schema::Table::Virtual(_))))
        .then_some(columns)
    } else {
        None
    };
    Ok(Source {
        table: table.clone(),
        alias: alias
            .as_ref()
            .map_or(name.name.as_str(), |a| a.name().as_str())
            .into(),
        collection,
        derived,
        derived_logical: false,
        derived_physical: None,
        native_expression_collations: Default::default(),
        native_collations: Default::default(),
        consumed: Default::default(),
    })
}
fn native_column_collation(expr: &Expr) -> bool {
    match expr {
        Expr::Collate(_, _) => true,
        Expr::Unary(UnaryOperator::Positive, value) => native_column_collation(value),
        Expr::Parenthesized(values) if values.len() == 1 => native_column_collation(&values[0]),
        _ => false,
    }
}

fn membership_column(expr: &Expr) -> bool {
    match expr {
        Expr::Unary(UnaryOperator::Positive, _) => false,
        Expr::Collate(value, _) => membership_column(value),
        Expr::Parenthesized(values) if values.len() == 1 => membership_column(&values[0]),
        _ => native_column_reference(expr),
    }
}

fn native_column_reference(expr: &Expr) -> bool {
    match expr {
        Expr::Id(_)
        | Expr::Name(_)
        | Expr::Qualified(_, _)
        | Expr::DoublyQualified(_, _, _)
        | Expr::Column { .. }
        | Expr::RowId { .. } => true,
        Expr::Collate(value, _) | Expr::Unary(UnaryOperator::Positive, value) => {
            native_column_reference(value)
        }
        Expr::Parenthesized(values) if values.len() == 1 => native_column_reference(&values[0]),
        _ => false,
    }
}

// Alias values are already lowered. Convert their native result without walking
// through generated typed protocol calls a second time.
fn native_alias_key(expr: &mut Expr) -> Result<bool> {
    match expr {
        Expr::Collate(value, _) | Expr::Unary(UnaryOperator::Positive, value) => {
            return native_alias_key(value)
        }
        Expr::Parenthesized(values) if values.len() == 1 => {
            return native_alias_key(&mut values[0])
        }
        Expr::Id(_)
        | Expr::Name(_)
        | Expr::Qualified(_, _)
        | Expr::DoublyQualified(_, _, _)
        | Expr::Column { .. }
        | Expr::RowId { .. } => return Ok(false),
        _ => {}
    }
    let cast_type = match expr {
        Expr::Cast { type_name, .. } => Some(type_name.clone()),
        _ => None,
    };
    *expr = expression(&format!("__fastdb_unwrap(__fastdb_pack({expr}))"))?;
    if let Some(type_name) = cast_type {
        *expr = Expr::Cast {
            expr: Box::new(expr.clone()),
            type_name,
        };
    }
    Ok(true)
}
fn blob_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(Literal::Blob(_)) => true,
        Expr::Parenthesized(es) if es.len() == 1 => blob_literal(&es[0]),
        _ => false,
    }
}
// SQL blob literals denote binary values, never pre-encoded record identity.
// Apply the same scalar-key representation used by typed fields and parameters.
fn index_literal(expr: &Expr, params: &Parameters) -> Result<Expr> {
    let mut binary_parameter = false;
    let mut probe = expr.clone();
    turso_core::walk_expr_mut(&mut probe, &mut |value| {
        if let Expr::Variable(var) = value {
            let name = var
                .name
                .as_ref()
                .map_or_else(|| format!("?{}", var.index), |name| name.to_string());
            binary_parameter |= matches!(params.get(&name), Some(Value::Binary(_)));
        }
        Ok(turso_core::WalkControl::Continue)
    })?;
    if blob_literal(expr) || binary_parameter {
        expression(&format!(
            "__fastdb_unwrap(__fastdb_pack({}))",
            order_base(expr)
        ))
    } else {
        Ok(expr.clone())
    }
}

fn constant(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_) | Expr::Variable(_) => true,
        Expr::Unary(_, e) => constant(e),
        Expr::Parenthesized(es) if es.len() == 1 => constant(&es[0]),
        Expr::FunctionCall { name, args, .. } if name.as_str() == "__fastdb_record_value" => {
            args.iter().all(|e| constant(e))
        }
        _ => false,
    }
}
fn indexed_filter(
    scope: &Scope,
    source_index: usize,
    predicate: &Expr,
) -> Result<Option<(crate::Index, Expr)>> {
    if let Expr::Parenthesized(es) = predicate {
        if es.len() == 1 {
            return indexed_filter(scope, source_index, &es[0]);
        }
    }
    if let Expr::Binary(lhs, Operator::And, rhs) = predicate {
        if let Some(candidate) = indexed_filter(scope, source_index, lhs)? {
            return Ok(Some(candidate));
        }
        return indexed_filter(scope, source_index, rhs);
    }
    // Null-accepting predicates cannot generally move below an outer join:
    // filtering matched rows can manufacture new NULL-extended rows.
    if scope.sources.len() == 1 {
        let field = match predicate {
            Expr::IsNull(field) => Some((field.as_ref(), false)),
            Expr::NotNull(field) => Some((field.as_ref(), true)),
            Expr::Binary(field, op @ (Operator::Is | Operator::IsNot), value)
                if matches!(value.as_ref(), Expr::Literal(Literal::Null)) =>
            {
                Some((field.as_ref(), *op == Operator::IsNot))
            }
            Expr::Binary(value, op @ (Operator::Is | Operator::IsNot), field)
                if matches!(value.as_ref(), Expr::Literal(Literal::Null)) =>
            {
                Some((field.as_ref(), *op == Operator::IsNot))
            }
            _ => None,
        };
        if let Some((mut field, not)) = field {
            while let Expr::Parenthesized(es) = field {
                if es.len() != 1 {
                    break;
                }
                field = &es[0];
            }
            if let Some((i, path)) = scope.field(field)? {
                if i == source_index {
                    if let Some(index) = scope.sources[i]
                        .collection
                        .as_ref()
                        .and_then(|c| c.indexes.iter().find(|idx| idx.path == path))
                    {
                        let key = Box::new(expression("i.key")?);
                        let filter = if not {
                            Expr::NotNull(key)
                        } else {
                            Expr::IsNull(key)
                        };
                        return Ok(Some((index.clone(), filter)));
                    }
                }
            }
        }
    }
    let candidates = match predicate {
        Expr::Binary(lhs, Operator::Equals, rhs) => vec![
            (lhs.as_ref(), vec![rhs.as_ref()], false),
            (rhs.as_ref(), vec![lhs.as_ref()], false),
        ],
        Expr::InList {
            lhs,
            rhs,
            not: false,
        } if !rhs.is_empty() => {
            vec![(lhs.as_ref(), rhs.iter().map(|e| e.as_ref()).collect(), true)]
        }
        _ => return Ok(None),
    };
    for (mut field, keys, membership) in candidates {
        if !keys.iter().all(|key| constant(key)) {
            continue;
        }
        while let Expr::Parenthesized(es) = field {
            if es.len() != 1 {
                break;
            }
            field = &es[0];
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
                let key = Box::new(expression("i.key")?);
                let filter = if membership {
                    Expr::InList {
                        lhs: key,
                        rhs: keys
                            .into_iter()
                            .map(|e| index_literal(e, &scope.params).map(Box::new))
                            .collect::<Result<_>>()?,
                        not: false,
                    }
                } else {
                    Expr::Binary(
                        key,
                        Operator::Equals,
                        Box::new(index_literal(keys[0], &scope.params)?),
                    )
                };
                return Ok(Some((index.clone(), filter)));
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
    if source.derived.is_some() {
        *table = source.table.clone();
        return Ok(());
    }
    let Some(c) = &source.collection else {
        return Ok(());
    };
    if let Some((index, filter)) = candidate {
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
        *predicate = Box::new(filter);
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
fn outer_collation(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Collate(_, name) => Some(name.as_str()),
        Expr::Parenthesized(values) if values.len() == 1 => outer_collation(&values[0]),
        _ => None,
    }
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
// Set membership uses SQL scalar keys; result representatives retain their
// original FastDB encoding. Materialization keeps volatile projections shared
// between the key query and representative recovery.
fn lower_set_operations(
    definitions: &mut Vec<String>,
    arms: &[String],
    columns: &[String],
    compounds: &[CompoundSelect],
) -> String {
    let names = columns.join(",");
    for (index, arm) in arms.iter().enumerate() {
        definitions.push(format!(
            "__fastdb_set_arm{index}({names}) AS MATERIALIZED ({arm})"
        ));
    }
    let mut left = "__fastdb_set_arm0".to_owned();
    let keys = |table: &str| {
        format!(
            "SELECT {} FROM {table}",
            columns
                .iter()
                .map(|column| format!("__fastdb_unwrap({column})"))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    for (index, compound) in compounds.iter().enumerate() {
        let right = format!("__fastdb_set_arm{}", index + 1);
        let result = format!("__fastdb_set_result{index}");
        if compound.operator == CompoundOperator::UnionAll {
            definitions.push(format!("{result}({names}) AS MATERIALIZED (SELECT * FROM {left} UNION ALL SELECT * FROM {right})"));
        } else {
            let key_table = format!("__fastdb_set_keys{index}");
            definitions.push(format!(
                "{key_table}({names}) AS MATERIALIZED ({} {} {})",
                keys(&left),
                compound.operator,
                keys(&right)
            ));
            let candidates = format!("__fastdb_set_candidates{index}");
            let source = if compound.operator == CompoundOperator::Union {
                format!("SELECT * FROM {left} UNION ALL SELECT * FROM {right}")
            } else {
                format!("SELECT * FROM {left}")
            };
            definitions.push(format!("{candidates}({names}) AS MATERIALIZED ({source})"));
            let outputs = columns
                .iter()
                .map(|column| format!("l.{column}"))
                .collect::<Vec<_>>()
                .join(",");
            let grouping = columns
                .iter()
                .map(|column| format!("__fastdb_unwrap(l.{column})"))
                .collect::<Vec<_>>()
                .join(",");
            let matching = columns
                .iter()
                .map(|column| format!("__fastdb_unwrap(l.{column}) IS k.{column}"))
                .collect::<Vec<_>>()
                .join(" AND ");
            definitions.push(format!("{result}({names}) AS MATERIALIZED (SELECT {outputs} FROM {candidates} l JOIN {key_table} k ON {matching} GROUP BY {grouping})"));
        }
        left = result;
    }
    format!("SELECT * FROM {left}")
}

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
    outer_scope: Option<(&'a [Source], &'a UsingColumns)>,
    membership_namespace: usize,
    native_with: Option<&'a With>,
    restricted_native_clauses: bool,
    ctes: Option<&'a CteSources>,
    nested: bool,
    expression_subquery: bool,
    trusted: bool,
    ignore_unused: bool,
    positional: bool,
    snapshot: Option<Option<&'a crate::Document>>,
    guarded: bool,
    native_insert: Option<&'a Stmt>,
    result_limits: Option<crate::ResultLimits>,
}
// A single-execution lowering result. Typed parameters may already be embedded
// in command; this is not a reusable prepared statement with replaceable binds.
struct LoweredSelect {
    command: Cmd,
    typed: Vec<bool>,
    fetched: Vec<bool>,
    names: Vec<String>,
    consumed: std::collections::BTreeSet<String>,
    ignore_unused: bool,
    explain: bool,
    native_insert: bool,
}
impl Connection {
    /// Execute one SQL SELECT and return its primary engine statement counters.
    /// Catalog/lowering queries and Rust decoding are excluded. Forward-fetch
    /// target counters are separate; errors do not return partial metrics.
    pub fn profile_select(&self, sql: &str, params: &Parameters) -> Result<crate::ProfiledQuery> {
        crate::parser_stack(|| self.profile_select_inner(sql, params, None))
    }
    /// Execute one SELECT with limits on retained result rows and logical payload.
    /// FETCH expansion shares the payload budget. Engine working memory is excluded.
    pub fn select_with_limits(
        &self,
        sql: &str,
        params: &Parameters,
        limits: crate::ResultLimits,
    ) -> Result<QueryResult> {
        self.profile_select_with_limits(sql, params, limits)
            .map(|profile| profile.result)
    }
    /// Profile a SELECT using the same result limits as `select_with_limits`.
    pub fn profile_select_with_limits(
        &self,
        sql: &str,
        params: &Parameters,
        limits: crate::ResultLimits,
    ) -> Result<crate::ProfiledQuery> {
        crate::parser_stack(|| self.profile_select_inner(sql, params, Some(limits)))
    }
    fn profile_select_inner(
        &self,
        sql: &str,
        params: &Parameters,
        limits: Option<crate::ResultLimits>,
    ) -> Result<crate::ProfiledQuery> {
        let fastql_parser::Statement::Sql(sql) = fastql_parser::parse(sql)? else {
            return Err(Error::Unsupported(
                "profiling requires one SQL SELECT".into(),
            ));
        };
        let expanded = expand_paths(&expand_records(&sql)?)?;
        if !matches!(parsed(&expanded)?, Cmd::Stmt(Stmt::Select(_))) {
            return Err(Error::Unsupported(
                "profiling requires one SQL SELECT".into(),
            ));
        }
        let has_fetch = fastql_parser::tokenize(&expanded)?
            .iter()
            .any(|t| t.kind == fastql_parser::Kind::Word && t.text == "__fastdb_fetch");
        let execute = || match self.lower_collection_select(
            &sql,
            &expanded,
            params,
            SelectOptions::default(),
        )? {
            Some(plan) => self.execute_lowered_profiled_with_limits(plan, params, limits),
            None => self.native_profiled_with_limits(&sql, params, limits),
        };
        if has_fetch {
            self.atomic(execute)
        } else {
            execute()
        }
    }

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
    pub(crate) fn native_insert_source(
        &self,
        sql: &str,
        params: &Parameters,
        insert: &Stmt,
        restricted_native_clauses: bool,
        limits: Option<crate::ResultLimits>,
    ) -> Result<Option<QueryResult>> {
        self.collection_select_options(
            sql,
            params,
            SelectOptions {
                restricted_native_clauses,
                trusted: true,
                positional: true,
                native_insert: Some(insert),
                result_limits: limits,
                ..Default::default()
            },
        )
    }
    pub(crate) fn returning_rows(
        &self,
        table: &QualifiedName,
        columns: &[ResultColumn],
        documents: Vec<crate::Document>,
        params: &Parameters,
        limits: Option<crate::ResultLimits>,
    ) -> Result<QueryResult> {
        let affected = documents.len() as i64;
        if columns.is_empty() {
            return Ok(QueryResult::command(affected));
        }
        if matches!(columns, [ResultColumn::Star]) {
            let columns = vec!["document".into()];
            let mut budget = crate::budget::ResultBudget::new(limits, &columns)?;
            let mut rows = Vec::new();
            for document in documents {
                let row = vec![Value::Object(document)];
                budget.row(&row)?;
                rows.push(row);
            }
            return Ok(QueryResult {
                columns,
                rows,
                affected,
            });
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
        let mut budget = None;
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
            if budget.is_none() {
                budget = Some(crate::budget::ResultBudget::new(limits, &row.columns)?);
            }
            let output = result.get_or_insert_with(|| QueryResult {
                columns: row.columns.clone(),
                rows: Vec::new(),
                affected,
            });
            for row in row.rows {
                budget.as_mut().expect("projection budget").row(&row)?;
                output.rows.push(row);
            }
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
        let limits = options.result_limits;
        let Some(plan) = self.lower_collection_select(sql, &expanded, params, options)? else {
            return Ok(None);
        };
        if limits.is_some() {
            self.execute_lowered_profiled_with_limits(plan, params, limits)
                .map(|profile| Some(profile.result))
        } else {
            self.execute_lowered_select(plan, params).map(Some)
        }
    }
    fn correlate_source_free_expression(
        &self,
        value: &mut Expr,
        correlation_sources: &[Source],
        params: &Parameters,
        ctes: &CteSources,
        outer_scope: Option<&Scope>,
    ) -> Result<()> {
        let mut failure = None;
        turso_core::walk_expr_mut(value, &mut |expr| {
            // These compiler accessors already bind an outer value. Their
            // physical column arguments must not be rebound as document paths.
            if matches!(expr, Expr::FunctionCall { name, .. }
                if matches!(name.as_str(), "__fastdb_value" | "__fastdb_correlated_value"))
            {
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            if let Expr::InSelect { lhs, .. } = expr {
                if let Err(error) = self.correlate_source_free_expression(
                    lhs,
                    correlation_sources,
                    params,
                    ctes,
                    outer_scope,
                ) {
                    failure = Some(error);
                    return Ok(turso_core::WalkControl::SkipChildren);
                }
            }
            let exists = matches!(expr, Expr::Exists(_));
            if let Expr::Subquery(query) | Expr::Exists(query) | Expr::InSelect { rhs: query, .. } =
                expr
            {
                if let Err(error) = self.correlate_collection_inner(
                    query,
                    correlation_sources,
                    params,
                    ctes,
                    exists,
                    outer_scope.is_some(),
                ) {
                    failure = Some(error);
                }
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            if let Some(outer_scope) = outer_scope {
                let replacement = outer_scope.field(expr).and_then(|field| {
                    field
                        .map(|(i, path)| {
                            let value = outer_scope.accessor(i, &path, true)?;
                            if outer_scope.sources[i].derived.is_some() {
                                expression(&format!("__fastdb_correlated_value({value})"))
                            } else {
                                Ok(value)
                            }
                        })
                        .transpose()
                });
                match replacement {
                    Ok(Some(value)) => {
                        *expr = value;
                        return Ok(turso_core::WalkControl::SkipChildren);
                    }
                    Err(error) => failure = Some(error),
                    Ok(None) => {}
                }
            }
            Ok(turso_core::WalkControl::Continue)
        })?;
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(())
    }

    fn bind_correlated_operand(value: &mut Expr, scope: &Scope) -> Result<()> {
        let mut rewrite_error = None;
        turso_core::walk_expr_mut(value, &mut |value| {
            if let Expr::InSelect { lhs, .. } = value {
                if let Err(error) = Self::bind_correlated_operand(lhs, scope) {
                    rewrite_error = Some(error);
                }
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            if matches!(
                value,
                Expr::Subquery(_) | Expr::Exists(_) | Expr::InSelect { .. }
            ) {
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            match scope.field(value).and_then(|field| {
                field
                    .map(|(i, path)| {
                        let value = scope.accessor(i, &path, true)?;
                        if scope.sources[i].derived.is_some() {
                            expression(&format!("__fastdb_correlated_value({value})"))
                        } else {
                            Ok(value)
                        }
                    })
                    .transpose()
            }) {
                Ok(Some(rewritten)) => {
                    *value = rewritten;
                    return Ok(turso_core::WalkControl::SkipChildren);
                }
                Err(error) => {
                    rewrite_error = Some(error);
                }
                Ok(None) => {}
            }
            Ok(turso_core::WalkControl::Continue)
        })?;
        if let Some(error) = rewrite_error {
            return Err(error);
        }
        Ok(())
    }

    fn correlate_collection_inner(
        &self,
        inner: &mut Select,
        correlation_sources: &[Source],
        params: &Parameters,
        ctes: &CteSources,
        exists: bool,
        logical_parent: bool,
    ) -> Result<()> {
        if !correlation_sources.iter().any(Source::logical) {
            return Ok(());
        }
        if inner.with.is_none()
            && !inner.body.compounds.is_empty()
            && !unordered_compound_pagination(inner)
        {
            for arm in std::iter::once(&mut inner.body.select)
                .chain(inner.body.compounds.iter_mut().map(|arm| &mut arm.select))
            {
                let mut single = Select {
                    with: None,
                    body: SelectBody {
                        select: arm.clone(),
                        compounds: Vec::new(),
                    },
                    order_by: Vec::new(),
                    limit: None,
                };
                preserve_compound_column_names(&mut single);
                self.correlate_collection_inner(
                    &mut single,
                    correlation_sources,
                    params,
                    ctes,
                    exists,
                    true,
                )?;
                *arm = single.body.select;
            }
            return Ok(());
        }
        if let Some(with) = &mut inner.with {
            if !with.recursive {
                for cte in &mut with.ctes {
                    self.correlate_collection_inner(
                        &mut cte.select,
                        correlation_sources,
                        params,
                        ctes,
                        false,
                        false,
                    )?;
                }
            }
        }
        // A source-free scalar wrapper introduces no table aliases. Carry
        // the enclosing logical scope into its projected and filtering subqueries.
        if inner.with.is_none()
            && inner.body.compounds.is_empty()
            && matches!(&inner.body.select, OneSelect::Select { from: None, .. })
        {
            let sql = Cmd::Stmt(Stmt::Select(inner.clone())).to_string();
            let logical = logical_parent
                || fastql_parser::tokenize(&sql)?.iter().any(|token| {
                    (token.kind == fastql_parser::Kind::Word
                        && token.text.starts_with("__fastdb_")
                        && token.text != "__fastdb_path")
                        || (token.kind == fastql_parser::Kind::Parameter
                            && params.get(&token.text).is_some_and(|value| {
                                matches!(
                                    value,
                                    Value::Boolean(_)
                                        | Value::Record(_)
                                        | Value::Object(_)
                                        | Value::Array(_)
                                        | Value::Vector(_)
                                        | Value::Binary(_)
                                )
                            }))
                });
            let outer_scope = Scope {
                using: UsingColumns::default(),
                qualified_only: true,
                expression_subqueries: Default::default(),
                sources: correlation_sources.to_vec(),
                params: params.clone(),
                consumed: Default::default(),
                fetched_aliases: Default::default(),
                standalone_aliases: Default::default(),
            };
            if let OneSelect::Select {
                from: None,
                columns,
                where_clause,
                window_clause,
                ..
            } = &mut inner.body.select
            {
                let values = columns
                    .iter_mut()
                    .filter_map(|column| match column {
                        ResultColumn::Expr(value, _) => Some(value),
                        _ => None,
                    })
                    .chain(where_clause.iter_mut())
                    .chain(inner.order_by.iter_mut().map(|sort| &mut sort.expr))
                    .chain(window_clause.iter_mut().flat_map(|definition| {
                        definition.window.partition_by.iter_mut().chain(
                            definition
                                .window
                                .order_by
                                .iter_mut()
                                .map(|sort| &mut sort.expr),
                        )
                    }));
                for value in values {
                    self.correlate_source_free_expression(
                        value,
                        correlation_sources,
                        params,
                        ctes,
                        logical.then_some(&outer_scope),
                    )?;
                }
            }
        }
        if let OneSelect::Select {
            from: Some(from), ..
        } = &inner.body.select
        {
            let tables = std::iter::once(&from.select).chain(from.joins.iter().map(|j| &j.table));
            let mut local = Vec::new();
            for (position, table) in tables.enumerate() {
                if matches!(
                    table.as_ref(),
                    SelectTable::Table(..) | SelectTable::Select(..)
                ) {
                    local.push(source(
                        self,
                        table,
                        params,
                        ctes,
                        inner.with.as_ref(),
                        anonymous_source_alias(from, position),
                        false,
                    )?);
                }
            }
            let local_count = 1 + from.joins.len();
            let visible: Vec<_> = correlation_sources
                .iter()
                .filter(|outer| {
                    !local
                        .iter()
                        .any(|source| source.alias.eq_ignore_ascii_case(&outer.alias))
                })
                .cloned()
                .collect();
            // A native parent can become logical when a nested compound binds
            // outer document values. Bind the parent's outer operands as well.
            let mut nested_logical = false;
            if let OneSelect::Select {
                columns,
                where_clause,
                group_by,
                from,
                window_clause,
                ..
            } = &mut inner.body.select
            {
                let mut values: Vec<_> = columns
                    .iter_mut()
                    .filter_map(|column| match column {
                        ResultColumn::Expr(value, _) => Some(value),
                        _ => None,
                    })
                    .collect();
                values.extend(where_clause.iter_mut());
                if let Some(group) = group_by {
                    values.extend(group.exprs.iter_mut());
                    values.extend(group.having.iter_mut());
                }
                if let Some(from) = from {
                    for join in &mut from.joins {
                        if let Some(JoinConstraint::On(value)) = &mut join.constraint {
                            values.push(value);
                        }
                    }
                }
                for window in window_clause {
                    values.extend(window.window.partition_by.iter_mut());
                    values.extend(window.window.order_by.iter_mut().map(|sort| &mut sort.expr));
                }
                values.extend(inner.order_by.iter_mut().map(|sort| &mut sort.expr));
                for value in values {
                    let mut failure = None;
                    turso_core::walk_expr_mut(value, &mut |expr| {
                        let exists = matches!(expr, Expr::Exists(_));
                        let membership = matches!(expr, Expr::InSelect { .. });
                        if let Expr::Subquery(query)
                        | Expr::Exists(query)
                        | Expr::InSelect { rhs: query, .. } = expr
                        {
                            let before = query.to_string();
                            if let Err(error) = self.correlate_collection_inner(
                                query, &visible, params, ctes, exists, false,
                            ) {
                                failure = Some(error);
                            }
                            nested_logical |= query.to_string() != before;
                            return Ok(if membership {
                                turso_core::WalkControl::Continue
                            } else {
                                turso_core::WalkControl::SkipChildren
                            });
                        }
                        Ok(turso_core::WalkControl::Continue)
                    })?;
                    if let Some(error) = failure {
                        return Err(error);
                    }
                }
            }
            // A local WITH may hide the logical source behind a
            // CTE name. Probe lowering only (never execution) to
            // distinguish it from a wholly native inner query.
            let local_cte_logical = if inner.with.as_ref().is_some_and(|with| !with.recursive)
                && inner.body.compounds.is_empty()
            {
                let sql = Cmd::Stmt(Stmt::Select(inner.clone())).to_string();
                self.lower_collection_select(
                    &sql,
                    &sql,
                    params,
                    SelectOptions {
                        trusted: true,
                        nested: true,
                        positional: exists,
                        expression_subquery: true,
                        ctes: Some(ctes),
                        ..Default::default()
                    },
                )?
                .is_some()
            } else {
                false
            };
            if inner.with.as_ref().is_none_or(|with| !with.recursive)
                && inner.body.compounds.is_empty()
                && local.len() == local_count
                && if inner.with.is_some() {
                    local_cte_logical
                } else {
                    logical_parent || nested_logical || local.iter().any(Source::logical)
                }
            {
                let scope = Scope {
                    using: UsingColumns::default(),
                    qualified_only: true,
                    expression_subqueries: Default::default(),
                    sources: correlation_sources
                        .iter()
                        .filter(|outer| {
                            outer.logical()
                                && !local
                                    .iter()
                                    .any(|s| s.alias.eq_ignore_ascii_case(&outer.alias))
                        })
                        .cloned()
                        .collect(),
                    params: params.clone(),
                    consumed: Default::default(),
                    fetched_aliases: Default::default(),
                    standalone_aliases: Default::default(),
                };
                let OneSelect::Select {
                    columns,
                    where_clause,
                    group_by,
                    from,
                    window_clause,
                    ..
                } = &mut inner.body.select
                else {
                    unreachable!()
                };
                let mut values = Vec::new();
                for column in columns {
                    if let ResultColumn::Expr(value, _) = column {
                        values.push(value);
                    }
                }
                values.extend(where_clause.iter_mut());
                if let Some(group) = group_by {
                    values.extend(group.exprs.iter_mut());
                    values.extend(group.having.iter_mut());
                }
                if let Some(from) = from {
                    for join in &mut from.joins {
                        if let Some(JoinConstraint::On(value)) = &mut join.constraint {
                            values.push(value);
                        }
                    }
                }
                for window in window_clause {
                    values.extend(window.window.partition_by.iter_mut());
                    values.extend(window.window.order_by.iter_mut().map(|sort| &mut sort.expr));
                }
                values.extend(inner.order_by.iter_mut().map(|sort| &mut sort.expr));
                for value in values {
                    Self::bind_correlated_operand(value, &scope)?;
                }
            }
        }
        Ok(())
    }

    fn lower_collection_select(
        &self,
        sql: &str,
        expanded: &str,
        params: &Parameters,
        options: SelectOptions<'_>,
    ) -> Result<Option<LoweredSelect>> {
        let SelectOptions {
            outer_scope,
            membership_namespace,
            native_with,
            restricted_native_clauses,
            ctes: inherited_ctes,
            nested,
            expression_subquery,
            trusted,
            ignore_unused,
            positional,
            snapshot,
            guarded: _,
            native_insert,
            result_limits: _,
        } = options;
        // Validate user expressions before introducing any internal function or
        // storage name. The existing write guard continues covering other SQL.
        if !trusted {
            crate::guard::internal_names(sql)?;
        }
        let Ok(mut cmd) = parsed(expanded) else {
            return Ok(None);
        };
        let explain = matches!(cmd, Cmd::ExplainQueryPlan(_) | Cmd::Explain(_));
        let select = match &mut cmd {
            Cmd::Stmt(Stmt::Select(s))
            | Cmd::Explain(Stmt::Select(s))
            | Cmd::ExplainQueryPlan(Stmt::Select(s)) => s,
            _ => return Ok(None),
        };
        let mut ctes = inherited_ctes.cloned().unwrap_or_default();
        let mut cte_consumed = std::collections::BTreeSet::new();
        let mut cte_logical = false;
        if let Some(mut with) = select.with.take() {
            if with.recursive {
                return Ok(None);
            }
            let mut local_names = std::collections::BTreeSet::new();
            for cte in &with.ctes {
                let name = cte.tbl_name.as_str().to_ascii_lowercase();
                if !local_names.insert(name.clone()) {
                    return Ok(None);
                }
                ctes.insert(name, None);
            }
            let mut resolved_ctes = Vec::new();
            for index in 0..with.ctes.len() {
                let mut cte = with.ctes[index].clone();
                // The pinned resolver chooses an enclosing same-name CTE.
                // Preserve that choice before introducing additional WITH levels.
                let inherited_native = inherited_ctes
                    .and_then(|ctes| ctes.get(&cte.tbl_name.as_str().to_ascii_lowercase()))
                    .and_then(Option::as_ref)
                    .is_some_and(|source| !source.logical());
                if let Some(inherited) = native_with.filter(|_| inherited_native).and_then(|with| {
                    with.ctes.iter().find(|outer| {
                        outer
                            .tbl_name
                            .as_str()
                            .eq_ignore_ascii_case(cte.tbl_name.as_str())
                    })
                }) {
                    cte = inherited.clone();
                }

                let sql = Cmd::Stmt(Stmt::Select(cte.select.clone())).to_string();
                let preceding = With {
                    recursive: false,
                    ctes: resolved_ctes.clone(),
                };
                let plan = match self.lower_collection_select(
                    &sql,
                    &sql,
                    params,
                    SelectOptions {
                        trusted: true,
                        nested: true,
                        ctes: Some(&ctes),
                        native_with: Some(&preceding),
                        membership_namespace: index + 1,
                        ..Default::default()
                    },
                ) {
                    Ok(plan) => plan,
                    Err(Error::Unsupported(_)) => return Ok(None),
                    Err(e) => return Err(e),
                };
                let (columns, mut physical, logical, consumed) = if let Some(plan) = plan {
                    if plan.fetched.iter().any(|f| *f) {
                        return Err(unsupported("fetched CTE projections"));
                    }
                    let names = if cte.columns.is_empty() {
                        plan.names
                    } else {
                        if cte.columns.len() != plan.typed.len() {
                            return Err(Error::Validation("CTE column count mismatch".into()));
                        }
                        cte.columns
                            .iter()
                            .map(|c| c.col_name.as_str().to_ascii_lowercase())
                            .collect()
                    };
                    let physical = derived_physical_names(&names);
                    cte.columns = physical
                        .iter()
                        .map(|n| IndexedColumn {
                            col_name: Name::from_string(quote(n)),
                            collation_name: None,
                            order: None,
                        })
                        .collect();
                    let Cmd::Stmt(Stmt::Select(mut body)) = plan.command else {
                        unreachable!("CTE SELECT plan");
                    };
                    if cte.select.with.is_none() {
                        if let Some(generated) = body.with.take() {
                            resolved_ctes.extend(generated.ctes);
                        }
                    }
                    cte.select = body;
                    (
                        names.into_iter().zip(plan.typed).collect(),
                        Some(physical),
                        true,
                        plan.consumed,
                    )
                } else {
                    // Inspect native output metadata with its preceding CTEs in
                    // scope; do not execute the native definition.
                    let Cmd::Stmt(Stmt::Select(mut probe)) =
                        parsed(&format!("SELECT * FROM {}", quote(cte.tbl_name.as_str())))?
                    else {
                        unreachable!();
                    };
                    probe.with = Some(With {
                        recursive: false,
                        ctes: resolved_ctes
                            .iter()
                            .cloned()
                            .chain(std::iter::once(cte.clone()))
                            .collect(),
                    });
                    let statement = match self.prepare(Cmd::Stmt(Stmt::Select(probe)).to_string()) {
                        Ok(s) => s,
                        Err(turso_core::LimboError::ParseError(_)) => return Ok(None),
                        Err(error) => return Err(error.into()),
                    };
                    let columns: Vec<_> = (0..statement.num_columns())
                        .map(|i| (statement.get_column_name(i).into_owned(), false))
                        .collect();
                    // Remaining collection index binds use binary comparison
                    // keys. Native CTE inputs need raw bytes, even when the same
                    // parameter is also used by a collection index predicate.
                    let mut rewritten = String::new();
                    let mut copied = 0;
                    let mut consumed = std::collections::BTreeSet::new();
                    for token in fastql_parser::tokenize(&sql)? {
                        if token.kind != fastql_parser::Kind::Parameter {
                            continue;
                        }
                        if let Some(Value::Binary(bytes)) = params.get(&token.text) {
                            rewritten.push_str(&sql[copied..token.start]);
                            rewritten.push_str("X'");
                            for byte in bytes {
                                rewritten.push_str(&format!("{byte:02x}"));
                            }
                            rewritten.push('\'');
                            copied = token.end;
                            consumed.insert(token.text);
                        }
                    }
                    if copied != 0 {
                        rewritten.push_str(&sql[copied..]);
                        let Cmd::Stmt(Stmt::Select(body)) = parsed(&rewritten)? else {
                            unreachable!();
                        };
                        cte.select = body;
                        cte.columns = columns
                            .iter()
                            .map(|(name, _)| IndexedColumn {
                                col_name: Name::from_string(quote(name)),
                                collation_name: None,
                                order: None,
                            })
                            .collect();
                    }
                    (columns, None, false, consumed)
                };
                cte_logical |= logical;
                cte_consumed.extend(consumed.iter().cloned());
                let name = cte.tbl_name.as_str().to_owned();
                let mut companion = None;
                let mut runtime_name = name.clone();
                if !logical {
                    let names = columns
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect::<Vec<_>>();
                    let runtime_columns = derived_physical_names(&names);
                    if runtime_columns != names {
                        // One companion per definition retains native CTE sharing
                        // across multiple mixed-query references.
                        runtime_name = format!("__fastdb_cte_columns_{index}");
                        while resolved_ctes.iter().any(|cte| cte.tbl_name.as_str().eq_ignore_ascii_case(&runtime_name))
                            || native_with.is_some_and(|with| with.ctes.iter().any(|cte| cte.tbl_name.as_str().eq_ignore_ascii_case(&runtime_name)))
                            || ctes.values().flatten().any(|source| matches!(&source.table, SelectTable::Table(name, _, _) if name.name.as_str().eq_ignore_ascii_case(&runtime_name)))
                        {
                            runtime_name.push('_');
                        }
                        let names = runtime_columns
                            .iter()
                            .map(|name| quote(name))
                            .collect::<Vec<_>>()
                            .join(",");
                        let Cmd::Stmt(Stmt::Select(mut wrapper)) = parsed(&format!(
                            "WITH {}({names}) AS (SELECT * FROM {}) SELECT 1",
                            quote(&runtime_name),
                            quote(&name)
                        ))?
                        else {
                            unreachable!("native CTE companion")
                        };
                        companion = wrapper.with.take().map(|mut with| with.ctes.remove(0));
                        physical = Some(runtime_columns);
                    }
                }
                let Cmd::Stmt(Stmt::Select(table_probe)) = parsed(&format!(
                    "SELECT * FROM {} AS {}",
                    quote(&runtime_name),
                    quote(&name)
                ))?
                else {
                    unreachable!();
                };
                let OneSelect::Select {
                    from: Some(from), ..
                } = table_probe.body.select
                else {
                    unreachable!();
                };
                ctes.insert(
                    name.to_ascii_lowercase(),
                    Some(Source {
                        table: *from.select,
                        alias: name,
                        collection: None,
                        derived: Some(columns),
                        derived_logical: logical,
                        derived_physical: physical,
                        native_collations: Default::default(),
                        native_expression_collations: Default::default(),
                        consumed,
                    }),
                );
                resolved_ctes.push(cte);
                if let Some(companion) = companion {
                    resolved_ctes.push(companion);
                }
            }
            with.ctes = resolved_ctes;
            select.with = Some(with);
        }
        if !select.body.compounds.is_empty() {
            return self
                .lower_compound(
                    select,
                    params,
                    cte_consumed,
                    cte_logical || (trusted && positional && native_insert.is_none()),
                    SelectOptions {
                        ctes: Some(&ctes),
                        ignore_unused,
                        ..Default::default()
                    },
                )?
                .map(|mut plan| {
                    plan.explain = explain;
                    if let Cmd::Stmt(statement) = plan.command {
                        plan.command = match &cmd {
                            Cmd::Explain(_) => Cmd::Explain(statement),
                            Cmd::ExplainQueryPlan(_) => Cmd::ExplainQueryPlan(statement),
                            _ => Cmd::Stmt(statement),
                        };
                    }
                    self.finish_select(plan, native_insert, restricted_native_clauses)
                })
                .transpose();
        }
        // Resolve outer collection fields before recursively lowering an inner
        // collection query. Otherwise its typed comparisons pack raw storage
        // IDs as strings, silently losing record identity.
        let source_free = matches!(&select.body.select, OneSelect::Select { from: None, .. });
        let inherited_scope = outer_scope.filter(|_| source_free);
        let mut correlation_sources = inherited_scope.map(|(sources, _)| sources.to_vec());
        let mut correlation_using =
            inherited_scope.map_or_else(UsingColumns::default, |(_, using)| using.clone());
        // Prepare nested scalar plans without executing them. Cache by the
        // original AST spelling so aliases and repeated lowering probes retain
        // type/parameter metadata; each occurrence still belongs to the engine.
        let mut expression_subqueries = ExpressionSubqueries::new();
        let mut native_expression_subqueries = ExpressionSubqueries::new();
        let mut subquery_error = None;
        let mut inputs = Vec::new();
        match &select.body.select {
            OneSelect::Values(rows) => inputs.extend(rows.iter().flatten().map(|e| *e.clone())),
            OneSelect::Select {
                columns,
                where_clause,
                group_by,
                from,
                window_clause,
                ..
            } => {
                inputs.extend(columns.iter().filter_map(|column| match column {
                    ResultColumn::Expr(expr, _) => Some(*expr.clone()),
                    _ => None,
                }));
                inputs.extend(where_clause.iter().map(|e| *e.clone()));
                for definition in window_clause {
                    inputs.extend(definition.window.partition_by.iter().map(|e| *e.clone()));
                    inputs.extend(definition.window.order_by.iter().map(|e| *e.expr.clone()));
                }
                if let Some(group) = group_by {
                    inputs.extend(group.exprs.iter().map(|e| *e.clone()));
                    inputs.extend(group.having.iter().map(|e| *e.clone()));
                }
                if let Some(from) = from {
                    for join in &from.joins {
                        if let Some(JoinConstraint::On(expr)) = &join.constraint {
                            inputs.push(*expr.clone());
                        }
                    }
                }
            }
        }
        inputs.extend(select.order_by.iter().map(|e| *e.expr.clone()));
        if let Some(limit) = &select.limit {
            inputs.push(*limit.expr.clone());
            inputs.extend(limit.offset.iter().map(|e| *e.clone()));
        }
        for mut input in inputs {
            turso_core::walk_expr_mut(&mut input, &mut |expr| {
                if let Expr::Subquery(inner)
                | Expr::Exists(inner)
                | Expr::InSelect { rhs: inner, .. } = &*expr
                {
                    let membership = matches!(expr, Expr::InSelect { .. });
                    let control = if membership {
                        turso_core::WalkControl::Continue
                    } else {
                        turso_core::WalkControl::SkipChildren
                    };
                    if subquery_error.is_some()
                        || expression_subqueries.contains_key(&expr.to_string())
                    {
                        return Ok(control);
                    }
                    let exists = matches!(expr, Expr::Exists(_));
                    let result = (|| -> Result<()> {
                        if correlation_sources.is_none() {
                            let mut resolved = Vec::new();
                            if let OneSelect::Select {
                                from: Some(from), ..
                            } = &select.body.select
                            {
                                for (position, table) in std::iter::once(&from.select)
                                    .chain(from.joins.iter().map(|j| &j.table))
                                    .enumerate()
                                {
                                    if matches!(
                                        table.as_ref(),
                                        SelectTable::Table(..) | SelectTable::Select(..)
                                    ) {
                                        resolved.push(source(
                                            self,
                                            table,
                                            params,
                                            &ctes,
                                            select.with.as_ref().or(native_with),
                                            anonymous_source_alias(from, position),
                                            from.joins.iter().any(|join| {
                                                matches!(
                                                    join.constraint,
                                                    Some(JoinConstraint::Using(_))
                                                ) || matches!(join.operator, JoinOperator::TypedJoin(Some(kind)) if kind.contains(JoinType::NATURAL))
                                            }),
                                        )?);
                                    }
                                }
                            }
                            if let OneSelect::Select {
                                from: Some(from), ..
                            } = &select.body.select
                            {
                                // Partial correlation sources omit unsupported table
                                // forms; their positions cannot represent USING sides.
                                if resolved.len() == from.joins.len() + 1 {
                                    let mut probe = from.clone();
                                    correlation_using = prepare_using(Some(&mut probe), &resolved)?;
                                }
                            }
                            correlation_sources = Some(resolved);
                        }
                        let correlation_sources = correlation_sources.as_ref().unwrap();
                        let mut inner = inner.clone();
                        qualify_correlated_using(
                            self,
                            params,
                            &ctes,
                            &mut inner,
                            correlation_sources,
                            &correlation_using,
                        )?;
                        self.correlate_collection_inner(
                            &mut inner,
                            correlation_sources,
                            params,
                            &ctes,
                            exists,
                            false,
                        )?;
                        let sql = Cmd::Stmt(Stmt::Select(inner.clone())).to_string();
                        // Typed RHS parameters must not select an isolated compound
                        // plan before the native correlation scope binds outer fields.
                        let mut direct_membership =
                            membership && direct_source_free_membership(&inner);
                        let mut distinct_membership = direct_membership
                            && inner
                                .body
                                .compounds
                                .iter()
                                .any(|arm| arm.operator != CompoundOperator::UnionAll);
                        if inner.with.is_none() && inner.body.compounds.is_empty() {
                            if let OneSelect::Select {
                                columns,
                                where_clause,
                                ..
                            } = &inner.body.select
                            {
                                for value in columns
                                    .iter()
                                    .filter_map(|column| match column {
                                        ResultColumn::Expr(value, _) => Some(value.as_ref()),
                                        _ => None,
                                    })
                                    .chain(where_clause.iter().map(|value| value.as_ref()))
                                {
                                    let mut value = value.clone();
                                    turso_core::walk_expr_mut(&mut value, &mut |expr| {
                                        if let Expr::InSelect { rhs, .. } = expr {
                                            direct_membership |= direct_source_free_membership(rhs);
                                            distinct_membership |=
                                                direct_source_free_membership(rhs)
                                                    && rhs.body.compounds.iter().any(|arm| {
                                                        arm.operator != CompoundOperator::UnionAll
                                                    });
                                        }
                                        Ok(turso_core::WalkControl::Continue)
                                    })?;
                                }
                            }
                        }
                        direct_membership &= correlation_sources.iter().any(Source::logical);
                        direct_membership &= distinct_membership
                            || fastql_parser::tokenize(&sql)?.iter().any(|token| {
                                token.kind == fastql_parser::Kind::Parameter
                                    && params.get(&token.text).is_some_and(|value| {
                                        matches!(
                                            value,
                                            Value::Boolean(_)
                                                | Value::Record(_)
                                                | Value::Array(_)
                                                | Value::Object(_)
                                                | Value::Vector(_)
                                        )
                                    })
                            });
                        if let OneSelect::Select {
                            from: Some(from), ..
                        } = &inner.body.select
                        {
                            for (position, table) in std::iter::once(&from.select)
                                .chain(from.joins.iter().map(|join| &join.table))
                                .enumerate()
                            {
                                if direct_membership {
                                    direct_membership &= !source(
                                        self,
                                        table,
                                        params,
                                        &ctes,
                                        select.with.as_ref().or(native_with),
                                        anonymous_source_alias(from, position),
                                        true,
                                    )?
                                    .logical();
                                }
                            }
                        }
                        let plan = if direct_membership {
                            None
                        } else {
                            self.lower_collection_select(
                                &sql,
                                &sql,
                                params,
                                SelectOptions {
                                    trusted: true,
                                    nested: true,
                                    positional: exists,
                                    expression_subquery: true,
                                    outer_scope: (!correlation_using.bindings.is_empty())
                                        .then_some((correlation_sources, &correlation_using)),
                                    ctes: Some(&ctes),
                                    ..Default::default()
                                },
                            )?
                        };
                        if let Some(mut plan) = plan {
                            let mut direct_local_scalar = false;
                            if !exists
                                && !membership
                                && inner.with.is_some()
                                && plan.typed == [true]
                            {
                                if let (Some(mut limit), Cmd::Stmt(Stmt::Select(prepared))) =
                                    (inner.limit.clone(), &mut plan.command)
                                {
                                    direct_local_scalar =
                                        literal_pagination(&mut limit.expr, params)?
                                            && match &mut limit.offset {
                                                Some(offset) => literal_pagination(offset, params)?,
                                                None => true,
                                            };
                                    if direct_local_scalar {
                                        for value in
                                            std::iter::once(&inner.limit.as_ref().unwrap().expr)
                                                .chain(inner.limit.as_ref().unwrap().offset.iter())
                                        {
                                            for token in
                                                fastql_parser::tokenize(&value.to_string())?
                                            {
                                                if token.kind == fastql_parser::Kind::Parameter {
                                                    plan.consumed.insert(token.text);
                                                }
                                            }
                                        }
                                        prepared.limit = Some(limit);
                                    }
                                }
                            }
                            if (!exists && plan.typed.len() != 1) || plan.fetched.iter().any(|f| *f)
                            {
                                return Err(unsupported(
                                    "scalar subqueries require one column; expression subqueries cannot FETCH",
                                ));
                            }
                            let output = if plan.typed.first() == Some(&true) {
                                "v"
                            } else {
                                "__fastdb_pack(v)"
                            };
                            let sql = plan.command.to_string();
                            let column = membership
                                && matches!(&inner.body.select, OneSelect::Select { columns, .. }
                                if matches!(columns.as_slice(), [ResultColumn::Expr(value, _)] if membership_column(value)));
                            let lowered = if exists {
                                let body = sql.trim().trim_end_matches(';');
                                // A scalar SELECT avoids premature correlated
                                // CTE preparation and source-free semi-join
                                // planning while retaining native EXISTS rules.
                                expression(&format!("(SELECT EXISTS({body}))"))?
                            } else if membership {
                                // Reusing this CTE name across nested membership
                                // plans can make the pinned planner recurse/crash.
                                let id = self
                                    .next_subquery_id
                                    .fetch_update(
                                        std::sync::atomic::Ordering::Relaxed,
                                        std::sync::atomic::Ordering::Relaxed,
                                        |id| id.checked_add(1),
                                    )
                                    .map_err(|_| {
                                        Error::Limit(
                                            "internal subquery identifiers exhausted".into(),
                                        )
                                    })?;
                                let members = format!("__fastdb_members_{id}");
                                let values =
                                    format!("SELECT __fastdb_unwrap({output}) AS v FROM {members}");
                                // A field has typeless column affinity, which differs
                                // from a function expression with no affinity. Restore
                                // a column boundary after decoding its comparison key.
                                let values = if column {
                                    format!("SELECT v FROM ({values}) AS __fastdb_member_values")
                                } else {
                                    values
                                };
                                expression(&format!(
                                    "(WITH {members}(v) AS ({}) {values})",
                                    sql.trim().trim_end_matches(';')
                                ))?
                            } else if direct_local_scalar {
                                expression(&format!("({})", sql.trim().trim_end_matches(';')))?
                            } else {
                                expression(&format!(
                            "(WITH __fastdb_scalar_result(v) AS ({}) SELECT {output} FROM __fastdb_scalar_result)",
                            sql.trim().trim_end_matches(';')
                        ))?
                            };
                            expression_subqueries.insert(
                                expr.to_string(),
                                (
                                    lowered,
                                    plan.consumed,
                                    if column {
                                        SubqueryAffinity::MembershipColumn
                                    } else {
                                        SubqueryAffinity::None
                                    },
                                ),
                            );
                        } else {
                            native_expression_subqueries.insert(
                                expr.to_string(),
                                (
                                    if membership {
                                        expression(&format!(
                                            "({})",
                                            sql.trim().trim_end_matches(';')
                                        ))?
                                    } else if exists {
                                        expr.clone()
                                    } else {
                                        expression(&format!("__fastdb_pack({expr})"))?
                                    },
                                    fastql_parser::tokenize(&sql)?
                                        .into_iter()
                                        .filter(|token| {
                                            token.kind == fastql_parser::Kind::Parameter
                                        })
                                        .map(|token| token.text.to_owned())
                                        .collect(),
                                    if membership {
                                        SubqueryAffinity::NativeMembership(
                                            "BINARY".into(),
                                            String::new(),
                                        )
                                    } else if exists {
                                        SubqueryAffinity::None
                                    } else {
                                        SubqueryAffinity::NativeScalar("BINARY".into())
                                    },
                                ),
                            );
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        subquery_error = Some(error);
                    }
                    return Ok(control);
                }
                Ok(turso_core::WalkControl::Continue)
            })?;
        }
        if let Some(error) = subquery_error {
            return Err(error);
        }
        if let OneSelect::Values(rows) = &mut select.body.select {
            let mut logical = !expression_subqueries.is_empty()
                || cte_logical
                || (trusted && positional && native_insert.is_none());
            for row in rows.iter_mut() {
                for value in row {
                    turso_core::walk_expr_mut(value, &mut |expr| {
                        match expr {
                            Expr::FunctionCall { name, .. }
                                if name.as_str().starts_with("__fastdb_") =>
                            {
                                logical = true
                            }
                            Expr::Variable(var) => {
                                let name = var
                                    .name
                                    .as_ref()
                                    .map_or_else(|| format!("?{}", var.index), |n| n.to_string());
                                logical |= params.get(&name).is_some_and(|value| {
                                    matches!(
                                        value,
                                        Value::Boolean(_)
                                            | Value::Record(_)
                                            | Value::Object(_)
                                            | Value::Array(_)
                                            | Value::Vector(_)
                                    )
                                });
                            }
                            _ => {}
                        }
                        Ok(turso_core::WalkControl::Continue)
                    })?;
                }
            }
            if !logical || native_insert.is_some() {
                return Ok(None);
            }
            if !select.body.compounds.is_empty()
                || !select.order_by.is_empty()
                || select.limit.is_some()
            {
                return Err(unsupported("compound or modified typed VALUES"));
            }
            let width = rows.first().map_or(0, Vec::len);
            let scope = Scope {
                using: UsingColumns::default(),
                qualified_only: false,
                expression_subqueries,
                sources: Vec::new(),
                params: params.clone(),
                consumed: std::cell::RefCell::new(cte_consumed),
                fetched_aliases: Default::default(),
                standalone_aliases: Default::default(),
            };
            for row in rows {
                if row.len() != width {
                    return Err(Error::Validation("VALUES row width mismatch".into()));
                }
                for value in row {
                    scope.typed(value)?;
                }
            }
            return Ok(Some(LoweredSelect {
                command: cmd,
                typed: vec![true; width],
                fetched: vec![false; width],
                names: (1..=width).map(|i| format!("column{i}")).collect(),
                consumed: scope.consumed.into_inner(),
                ignore_unused,
                explain,
                native_insert: false,
            }));
        }
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
            let first = match source(
                self,
                &from.select,
                params,
                &ctes,
                select.with.as_ref().or(native_with),
                anonymous_source_alias(from, 0),
                true,
            ) {
                Ok(s) => s,
                Err(Error::Unsupported(_)) => return Ok(None),
                Err(e) => return Err(e),
            };
            sources.push(first);
            for (position, join) in from.joins.iter().enumerate() {
                match source(
                    self,
                    &join.table,
                    params,
                    &ctes,
                    select.with.as_ref().or(native_with),
                    anonymous_source_alias(from, position + 1),
                    true,
                ) {
                    Ok(s) => sources.push(s),
                    Err(Error::Unsupported(_)) => return Ok(None),
                    Err(e) => return Err(e),
                }
            }
        }
        // A native source opts in for explicit logical projections. A binary
        // parameter used only by its WHERE clause must keep native affinity.
        let logical_input = if from.is_some() && nested {
            std::borrow::Cow::Owned(
                columns
                    .iter()
                    .filter_map(|column| match column {
                        ResultColumn::Expr(value, _) => Some(value.to_string()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(","),
            )
        } else {
            std::borrow::Cow::Borrowed(expanded)
        };
        let explicit_logical_expression = (from.is_none() || nested)
            && fastql_parser::tokenize(&logical_input)?
                .iter()
                .any(|token| {
                    (token.kind == fastql_parser::Kind::Word && token.text.starts_with("__fastdb_")
                        // A source-free nested path has no local field scope.
                        // Leave it for the enclosing correlation pass.
                        && !(nested && expression_subquery && from.is_none() && token.text == "__fastdb_path"))
                        || (token.kind == fastql_parser::Kind::Parameter
                            && params.get(&token.text).is_some_and(|value| {
                                matches!(
                                    value,
                                    Value::Boolean(_)
                                        | Value::Record(_)
                                        | Value::Object(_)
                                        | Value::Array(_)
                                        | Value::Vector(_)
                                ) || (expression_subquery && matches!(value, Value::Binary(_)))
                            }))
                });
        if from.is_none()
            && (!expression_subqueries.is_empty() || !native_expression_subqueries.is_empty())
        {
            if let Some((outer_sources, _)) = inherited_scope {
                sources.extend_from_slice(outer_sources);
            }
        }
        if (native_insert.is_some() || nested)
            && !cte_logical
            && expression_subqueries.is_empty()
            && !explicit_logical_expression
            && sources.iter().all(|source| !source.logical())
        {
            return Ok(None);
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
            && !cte_logical
            && expression_subqueries.is_empty()
            && sources.iter().all(|s| !s.logical())
            && expanded == sql
            && !standalone_typed_parameters
        {
            return Ok(None);
        }
        let using = if from.is_none() {
            inherited_scope.map_or_else(UsingColumns::default, |(_, using)| using.clone())
        } else {
            prepare_using(from.as_mut(), &sources)?
        };
        // The pinned engine does not expose outer membership CTEs while
        // preparing JOIN ON subqueries. Keep these RHS queries in place.
        let mut join_memberships = std::collections::BTreeSet::new();
        if let Some(from) = from.as_ref() {
            for join in &from.joins {
                if let Some(JoinConstraint::On(predicate)) = &join.constraint {
                    let mut predicate = *predicate.clone();
                    turso_core::walk_expr_mut(&mut predicate, &mut |expr| {
                        if matches!(expr, Expr::InSelect { .. }) {
                            join_memberships.insert(expr.to_string());
                        }
                        Ok(turso_core::WalkControl::Continue)
                    })?;
                }
            }
        }
        // Native expression queries do not opt an ordinary SQL statement into
        // logical lowering; preserve their values only once that route is chosen.
        let metadata_scopes: Vec<_> = native_with
            .into_iter()
            .chain(select.with.as_ref())
            .cloned()
            .collect();
        for (sql, (lowered, _, affinity)) in &mut native_expression_subqueries {
            if let Expr::Exists(mut inner) = expression(sql)? {
                qualify_correlated_using(self, params, &ctes, &mut inner, &sources, &using)?;
                *lowered = Expr::Exists(
                    native_correlated_predicate(
                        self,
                        &metadata_scopes,
                        &inner,
                        &sources,
                        false,
                        params,
                        NativeCorrelationMode::ScalarPagination,
                    )?
                    .0,
                );
                continue;
            }
            if !matches!(
                affinity,
                SubqueryAffinity::NativeScalar(_) | SubqueryAffinity::NativeMembership(_, _)
            ) {
                continue;
            }
            let mut inner = match expression(sql)? {
                Expr::Subquery(inner) | Expr::InSelect { rhs: inner, .. } => inner,
                _ => unreachable!(),
            };
            qualify_correlated_using(self, params, &ctes, &mut inner, &sources, &using)?;
            let mut collation = "BINARY".to_owned();
            let correlated = {
                let (mut probe, _) = native_correlated_predicate(
                    self,
                    &metadata_scopes,
                    &inner,
                    &sources,
                    true,
                    params,
                    if matches!(affinity, SubqueryAffinity::NativeMembership(_, _)) {
                        NativeCorrelationMode::Membership
                    } else {
                        NativeCorrelationMode::Native
                    },
                )?;
                let correlated = Cmd::Stmt(Stmt::Select(probe.clone())).to_string()
                    != Cmd::Stmt(Stmt::Select(inner.clone())).to_string();
                if probe.with.is_none() {
                    probe.with = select.with.clone();
                    if let Some(inherited) = native_with.filter(|with| !with.ctes.is_empty()) {
                        let query = Cmd::Stmt(Stmt::Select(probe)).to_string();
                        let Cmd::Stmt(Stmt::Select(wrapped)) = parsed(&format!(
                            "{inherited} SELECT * FROM ({})",
                            query.trim().trim_end_matches(';')
                        ))?
                        else {
                            unreachable!("native metadata scope")
                        };
                        probe = wrapped;
                    }
                }
                let statement = self.prepare(Cmd::Stmt(Stmt::Select(probe)).to_string())?;
                let program = statement.get_program();
                if let Some(column) = program.result_columns.first() {
                    let mut value = column.expr.clone();
                    let mut implicit = None;
                    let mut explicit = None;
                    turso_core::walk_expr_mut(&mut value, &mut |expr| {
                        match expr {
                            Expr::Collate(_, name) => {
                                explicit.get_or_insert_with(|| name.as_str().to_owned());
                                return Ok(turso_core::WalkControl::SkipChildren);
                            }
                            Expr::Column { table, column, .. } => {
                                if let Some((_, source)) =
                                    program.table_references.find_table_by_internal_id(*table)
                                {
                                    if let Some(column) = source.get_column_at(*column) {
                                        implicit.get_or_insert_with(|| column.collation().name());
                                    }
                                }
                            }
                            _ => {}
                        }
                        Ok(turso_core::WalkControl::Continue)
                    })?;
                    // Correlated scalar results are registers in the pinned engine:
                    // their projected collation does not propagate to the outer comparison.
                    if !correlated || matches!(affinity, SubqueryAffinity::NativeMembership(_, _)) {
                        collation = explicit.or(implicit).unwrap_or(collation);
                    }
                }
                correlated
            };
            let (runtime, typed_projection) = native_correlated_predicate(
                self,
                &metadata_scopes,
                &inner,
                &sources,
                false,
                params,
                if matches!(affinity, SubqueryAffinity::NativeScalar(_)) {
                    NativeCorrelationMode::ScalarPagination
                } else {
                    NativeCorrelationMode::Membership
                },
            )?;
            if typed_projection {
                let runtime_sql = Cmd::Stmt(Stmt::Select(runtime)).to_string();
                let runtime_sql = runtime_sql.trim().trim_end_matches(';');
                if matches!(affinity, SubqueryAffinity::NativeMembership(_, _)) {
                    let column = matches!(&inner.body.select, OneSelect::Select { columns, .. }
                        if matches!(columns.as_slice(), [ResultColumn::Expr(value, _)] if membership_column(value)));
                    let values = "SELECT __fastdb_unwrap(v) AS v FROM __fastdb_members";
                    let values = if column {
                        format!("SELECT v FROM ({values}) AS __fastdb_member_values")
                    } else {
                        values.into()
                    };
                    *lowered = expression(&format!(
                        "(WITH __fastdb_members(v) AS ({runtime_sql}) {values})"
                    ))?;
                    *affinity = if column {
                        SubqueryAffinity::MembershipColumn
                    } else {
                        SubqueryAffinity::None
                    };
                } else {
                    *lowered = expression(&format!("({runtime_sql})"))?;
                    *affinity = SubqueryAffinity::None;
                }
                continue;
            }
            if (correlated || join_memberships.contains(sql))
                && matches!(affinity, SubqueryAffinity::NativeMembership(_, _))
            {
                *lowered = Expr::Subquery(
                    native_correlated_predicate(
                        self,
                        &metadata_scopes,
                        &inner,
                        &sources,
                        false,
                        params,
                        NativeCorrelationMode::Membership,
                    )?
                    .0,
                );
                *affinity = SubqueryAffinity::NativeMembership(collation, String::new());
                continue;
            }
            *affinity = if matches!(affinity, SubqueryAffinity::NativeMembership(_, _)) {
                let mut suffix = select.with.as_ref().map_or(0, |with| with.ctes.len());
                let name = loop {
                    let name =
                        format!("__fastdb_membership_source_{membership_namespace}_{suffix}");
                    if !expanded.to_ascii_lowercase().contains(&name) {
                        break name;
                    }
                    suffix += 1;
                };
                let sql = Cmd::Stmt(Stmt::Select(inner)).to_string();
                let Cmd::Stmt(Stmt::Select(mut generated)) = parsed(&format!(
                    "WITH {name}(v) AS MATERIALIZED ({}) SELECT 1",
                    sql.trim().trim_end_matches(';')
                ))?
                else {
                    unreachable!()
                };
                let generated = generated.with.take().expect("membership WITH");
                if let Some(with) = &mut select.with {
                    with.ctes.extend(generated.ctes);
                } else {
                    select.with = Some(generated);
                }
                SubqueryAffinity::NativeMembership(collation, name)
            } else {
                let (runtime, _) = native_correlated_predicate(
                    self,
                    &metadata_scopes,
                    &inner,
                    &sources,
                    false,
                    params,
                    NativeCorrelationMode::ScalarPagination,
                )?;
                *lowered = expression(&format!(
                    "__fastdb_pack(({}))",
                    Cmd::Stmt(Stmt::Select(runtime))
                        .to_string()
                        .trim()
                        .trim_end_matches(';')
                ))?;
                SubqueryAffinity::NativeScalar(collation)
            };
        }
        expression_subqueries.extend(native_expression_subqueries);
        let distinct = !sources.is_empty() && matches!(distinctness, Some(Distinctness::Distinct));
        if distinct {
            *distinctness = None;
        }
        if !select.body.compounds.is_empty() {
            return Err(unsupported("compound SELECT"));
        }
        let consumed = sources
            .iter()
            .flat_map(|s| s.consumed.iter().cloned())
            .chain(cte_consumed)
            .collect();
        let mut scope = Scope {
            using,
            qualified_only: false,
            expression_subqueries,
            sources,
            params: params.clone(),
            consumed: std::cell::RefCell::new(consumed),
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
        *columns = expand_stars(self, &scope, columns, from.as_ref())?;
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
                    } else if let Some((index, path)) = &field {
                        let name = path.last().expect("nonempty path");
                        if path.len() == 1 {
                            scope.sources[*index]
                                .derived
                                .as_ref()
                                .and_then(|columns| {
                                    columns
                                        .iter()
                                        .find(|(column, _)| column.eq_ignore_ascii_case(name))
                                })
                                .map_or_else(|| name.clone(), |(column, _)| column.clone())
                        } else {
                            name.clone()
                        }
                    } else if let Some((_, column)) = match &expr {
                        Expr::Id(name) | Expr::Name(name) => scope
                            .using
                            .bindings
                            .get(&name.as_str().to_ascii_lowercase()),
                        _ => None,
                    } {
                        column.clone()
                    } else if let Expr::Qualified(qualifier, column) = &expr {
                        scope
                            .sources
                            .iter()
                            .find(|source| source.alias.eq_ignore_ascii_case(qualifier.as_str()))
                            .and_then(|source| source.derived.as_ref())
                            .and_then(|columns| {
                                columns
                                    .iter()
                                    .find(|(name, _)| name.eq_ignore_ascii_case(column.as_str()))
                            })
                            .map(|(name, _)| Ok(name.clone()))
                            .unwrap_or_else(|| public_expression_name(&expr))?
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
        if !scope.sources.is_empty() {
            if let Some(predicate) = where_clause {
                expand_projection_aliases(
                    predicate,
                    &original_columns,
                    false,
                    &scope.sources,
                    &mut scope.expression_subqueries,
                )?;
            }
        }
        let candidates = scope
            .sources
            .iter()
            .enumerate()
            .map(|(i, _)| {
                where_clause
                    .as_ref()
                    .map_or(Ok(None), |p| indexed_filter(&scope, i, p))
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
                        JoinConstraint::On(e) => {
                            expand_projection_aliases(
                                e,
                                &original_columns,
                                false,
                                &scope.sources,
                                &mut scope.expression_subqueries,
                            )?;
                            scope.sql_argument(e)?;
                        }
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
            scope.sql_argument(expr)?;
        }
        if let Some(group) = group_by {
            // Ordinals refer to the original expression: grouping the encoded
            // projection would give different numeric/null SQL semantics.
            for expr in &mut group.exprs {
                let ordinal = expand_group_position(expr, &original_columns)?;
                if !ordinal && !scope.sources.is_empty() {
                    expand_projection_aliases(
                        expr,
                        &original_columns,
                        true,
                        &scope.sources,
                        &mut scope.expression_subqueries,
                    )?;
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
                scope.sql_argument(expr)?;
                // The pinned engine can reuse an uninitialized group-key
                // register for a function key referenced only by HAVING.
                // Preserve projected expressions, including aggregate calls
                // nested in output arithmetic, so HAVING can reuse them.
                let mut projected_calls = std::collections::BTreeSet::new();
                for column in columns.iter() {
                    if let ResultColumn::Expr(value, _) = column {
                        let mut value = *value.clone();
                        turso_core::walk_expr_mut(&mut value, &mut |expr| {
                            if matches!(
                                expr,
                                Expr::Subquery(_) | Expr::Exists(_) | Expr::InSelect { .. }
                            ) {
                                return Ok(turso_core::WalkControl::SkipChildren);
                            }
                            // Aggregate names/arities follow the pinned engine;
                            // scalar MIN/MAX calls must still be rewritten.
                            let aggregate = match expr {
                                Expr::FunctionCall { name, args, .. } => {
                                    match name.as_str().to_ascii_lowercase().as_str() {
                                        "min" | "max" => args.len() == 1,
                                        "avg" | "count" | "sum" | "total" | "group_concat"
                                        | "string_agg" | "array_agg" | "json_group_array"
                                        | "jsonb_group_array" | "json_group_object"
                                        | "jsonb_group_object" => true,
                                        _ => false,
                                    }
                                }
                                Expr::FunctionCallStar { name, .. } => {
                                    name.as_str().eq_ignore_ascii_case("count")
                                }
                                _ => false,
                            };
                            if aggregate {
                                projected_calls.insert(aggregate_reuse_key(expr));
                                return Ok(turso_core::WalkControl::SkipChildren);
                            }
                            Ok(turso_core::WalkControl::Continue)
                        })?;
                    }
                }
                // These aliases use the same implementations and retain binary
                // payload semantics without matching the group-key expression.
                turso_core::walk_expr_mut(expr, &mut |value| {
                    if matches!(
                        value,
                        Expr::FunctionCall { .. } | Expr::FunctionCallStar { .. }
                    ) && projected_calls.contains(&aggregate_reuse_key(value))
                    {
                        return Ok(turso_core::WalkControl::SkipChildren);
                    }
                    if matches!(
                        value,
                        Expr::Subquery(_) | Expr::Exists(_) | Expr::InSelect { .. }
                    ) {
                        return Ok(turso_core::WalkControl::SkipChildren);
                    }
                    if let Expr::FunctionCall { name, .. } = value {
                        match name.as_str() {
                            "__fastdb_scalar" => {
                                *name = Name::exact("__fastdb_having_scalar".into())
                            }
                            "__fastdb_sql_scalar" => {
                                *name = Name::exact("__fastdb_having_sql_scalar".into())
                            }
                            _ => {}
                        }
                    }
                    Ok(turso_core::WalkControl::Continue)
                })?;
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
            let expression_index = original_columns.iter().position(|column| {
                matches!(column, ResultColumn::Expr(expr, _) if expr.as_ref() == order_base(&sorted.expr))
            });
            let alias_index = alias_index.or(expression_index);
            order_outputs.push(alias_index);
            if let Some(i) = alias_index {
                if fetched[i] {
                    return Err(unsupported("ordering on fetched values"));
                }
                // An expression match still contains logical source fields.
                // Reuse its lowered projection even when its result is native
                // (for example SUM), rather than leaving those fields unbound.
                if !typed[i] && (distinct || expression_index == Some(i)) {
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
        if let Some(limit) = &mut select.limit {
            // Pagination has no access to outer fields or projection aliases.
            let pagination = Scope {
                using: UsingColumns::default(),
                qualified_only: false,
                expression_subqueries: scope.expression_subqueries.clone(),
                sources: Vec::new(),
                params: params.clone(),
                consumed: Default::default(),
                fetched_aliases: Default::default(),
                standalone_aliases: Default::default(),
            };
            for value in std::iter::once(&mut limit.expr).chain(limit.offset.iter_mut()) {
                if expression_subquery {
                    let mut missing = None;
                    turso_core::walk_expr_mut(value, &mut |expr| {
                        if let Expr::Variable(var) = expr {
                            let name = var
                                .name
                                .as_ref()
                                .map_or_else(|| format!("?{}", var.index), |name| name.to_string());
                            if !params.contains_key(&name) {
                                missing = Some(name.clone());
                            }
                        }
                        Ok(turso_core::WalkControl::Continue)
                    })?;
                    if let Some(name) = missing {
                        return Err(Error::Parameter(name));
                    }
                }
                let mut probe = *value.clone();
                let mut logical = false;
                turso_core::walk_expr_mut(&mut probe, &mut |expr| {
                    if matches!(
                        expr,
                        Expr::Subquery(_) | Expr::Exists(_) | Expr::InSelect { .. }
                    ) {
                        logical |= pagination
                            .expression_subqueries
                            .contains_key(&expr.to_string());
                    }
                    Ok(turso_core::WalkControl::Continue)
                })?;
                if logical {
                    pagination.sql_argument(value)?;
                }
                if expression_subquery {
                    **value = expression(&format!("__fastdb_pagination_value({value})"))?;
                }
            }
            scope
                .consumed
                .borrow_mut()
                .extend(pagination.consumed.into_inner());
        }
        if distinct {
            if fetched.iter().any(|v| *v) {
                return Err(unsupported("DISTINCT on fetched documents"));
            }
            lower_distinct(select, &typed, &order_outputs)?;
        }
        self.finish_select(
            LoweredSelect {
                command: cmd,
                typed,
                fetched,
                names,
                consumed: scope.consumed.into_inner(),
                ignore_unused,
                explain,
                native_insert: false,
            },
            native_insert,
            restricted_native_clauses,
        )
        .map(Some)
    }
    fn lower_compound(
        &self,
        select: &Select,
        params: &Parameters,
        mut consumed: std::collections::BTreeSet<String>,
        mut logical: bool,
        options: SelectOptions<'_>,
    ) -> Result<Option<LoweredSelect>> {
        let ctes = options.ctes.expect("compound CTE scope");
        let arms = std::iter::once(&select.body.select)
            .chain(select.body.compounds.iter().map(|arm| &arm.select));
        let mut plans = Vec::new();
        for (arm_index, arm) in arms.enumerate() {
            let body = Select {
                with: None,
                body: SelectBody {
                    select: arm.clone(),
                    compounds: Vec::new(),
                },
                order_by: Vec::new(),
                limit: None,
            };
            let sql = Cmd::Stmt(Stmt::Select(body)).to_string();
            let detected = self.lower_collection_select(
                &sql,
                &sql,
                params,
                SelectOptions {
                    membership_namespace: arm_index + 1,
                    native_with: select.with.as_ref().or(options.native_with),
                    trusted: true,
                    nested: true,
                    ctes: Some(ctes),
                    ..Default::default()
                },
            )?;
            logical |= detected.is_some();
            for token in fastql_parser::tokenize(&sql)? {
                logical |=
                    token.kind == fastql_parser::Kind::Word && token.text.starts_with("__fastdb_");
                logical |= token.kind == fastql_parser::Kind::Parameter
                    && params.get(&token.text).is_some_and(|v| {
                        matches!(
                            v,
                            Value::Boolean(_)
                                | Value::Record(_)
                                | Value::Object(_)
                                | Value::Array(_)
                                | Value::Vector(_)
                        )
                    });
            }
            plans.push((sql, detected));
        }
        // Lower pagination independently of compound output names and arm scopes.
        let mut pagination = select.limit.clone();
        if let Some(limit) = &pagination {
            let Cmd::Stmt(Stmt::Select(mut probe)) = parsed("SELECT 1")? else {
                unreachable!("pagination SELECT")
            };
            probe.limit = Some(limit.clone());
            let sql = Cmd::Stmt(Stmt::Select(probe.clone())).to_string();
            if let Some(plan) = self.lower_collection_select(
                &sql,
                &sql,
                params,
                SelectOptions {
                    trusted: true,
                    nested: true,
                    ctes: Some(ctes),
                    ..Default::default()
                },
            )? {
                logical = true;
                consumed.extend(plan.consumed);
                let Cmd::Stmt(Stmt::Select(lowered)) = plan.command else {
                    unreachable!("pagination SELECT")
                };
                pagination = lowered.limit;
            }
        }
        if !logical {
            return Ok(None);
        }
        if let Some(with) = &select.with {
            for token in fastql_parser::tokenize(&with.to_string())? {
                if token.kind == fastql_parser::Kind::Parameter {
                    consumed.insert(token.text);
                }
            }
        }
        // Native pagination probes may return no logical plan. Their parameters
        // still belong to this compound and must be validated and propagated.
        if let Some(limit) = &select.limit {
            for value in std::iter::once(&limit.expr).chain(limit.offset.iter()) {
                for token in fastql_parser::tokenize(&value.to_string())? {
                    if token.kind == fastql_parser::Kind::Parameter {
                        consumed.insert(token.text);
                    }
                }
            }
        }
        for name in &consumed {
            if !params.contains_key(name) {
                return Err(Error::Parameter(name.clone()));
            }
        }
        let distinct = select
            .body
            .compounds
            .iter()
            .any(|arm| arm.operator != CompoundOperator::UnionAll);
        // Keep LIMIT/OFFSET on the compound itself: a CTE wrapper around each
        // arm can evaluate projections that native UNION ALL would skip.
        let direct_pagination =
            !distinct && select.order_by.is_empty() && direct_compound_pagination(select);
        let mut lowered = Vec::new();
        let mut definitions = Vec::new();
        let mut names = Vec::new();
        let mut order_names = Vec::new();
        for (index, (sql, detected)) in plans.into_iter().enumerate() {
            let mut plan = match detected {
                Some(plan) => plan,
                None => self
                    .lower_collection_select(
                        &sql,
                        &sql,
                        params,
                        SelectOptions {
                            trusted: true,
                            positional: true,
                            ctes: Some(ctes),
                            ..Default::default()
                        },
                    )?
                    .ok_or_else(|| unsupported("this typed compound arm"))?,
            };
            if plan.fetched.iter().any(|fetch| *fetch) {
                return Err(unsupported("fetched compound projections"));
            }
            if index == 0 {
                names = plan.names.clone();
            }
            if plan.names.len() != names.len() {
                return Err(Error::Validation("compound column count mismatch".into()));
            }
            order_names.push(plan.names.clone());
            consumed.extend(plan.consumed);
            let keys = (0..names.len())
                .map(|i| format!("__fastdb_v{i}"))
                .collect::<Vec<_>>();
            let values = keys
                .iter()
                .zip(&plan.typed)
                .map(|(key, typed)| {
                    if *typed {
                        key.clone()
                    } else {
                        format!("__fastdb_pack({key})")
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            if let Cmd::Stmt(Stmt::Select(arm)) = &mut plan.command {
                if let Some(with) = arm.with.take() {
                    definitions.extend(with.ctes.into_iter().map(|cte| cte.to_string()));
                }
            }
            if direct_pagination {
                let Cmd::Stmt(Stmt::Select(arm)) = &mut plan.command else {
                    unreachable!()
                };
                if !arm.body.compounds.is_empty() || !arm.order_by.is_empty() || arm.limit.is_some()
                {
                    return Err(unsupported("nested pagination in a UNION ALL arm"));
                }
                let OneSelect::Select { columns, .. } = &mut arm.body.select else {
                    return Err(unsupported("this paginated UNION ALL arm"));
                };
                for (column, typed) in columns.iter_mut().zip(&plan.typed) {
                    let ResultColumn::Expr(value, _) = column else {
                        unreachable!("expanded compound columns")
                    };
                    if !typed {
                        *value = Box::new(expression(&format!("__fastdb_pack({value})"))?);
                    }
                }
                lowered.push(
                    plan.command
                        .to_string()
                        .trim()
                        .trim_end_matches(';')
                        .to_owned(),
                );
                continue;
            }
            let sql = plan.command.to_string();
            let arm_name = format!("__fastdb_union_arm{index}");
            definitions.push(format!(
                "{arm_name}({}) AS {}({})",
                keys.join(","),
                if distinct { "MATERIALIZED " } else { "" },
                sql.trim().trim_end_matches(';')
            ));
            lowered.push(format!("SELECT {values} FROM {arm_name}"));
        }
        if direct_pagination {
            let sql = format!(
                "{}{}",
                if definitions.is_empty() {
                    String::new()
                } else {
                    format!("WITH {} ", definitions.join(","))
                },
                lowered.join(" UNION ALL ")
            );
            let Cmd::Stmt(Stmt::Select(mut result)) = parsed(&sql)? else {
                unreachable!()
            };
            if let Some(mut with) = select.with.clone() {
                if let Some(generated) = result.with.take() {
                    with.ctes.extend(generated.ctes);
                }
                result.with = Some(with);
            }
            result.limit = pagination;
            return Ok(Some(LoweredSelect {
                command: Cmd::Stmt(Stmt::Select(result)),
                typed: vec![true; names.len()],
                fetched: vec![false; names.len()],
                names,
                consumed,
                ignore_unused: options.ignore_unused,
                explain: false,
                native_insert: false,
            }));
        }
        let width = names.len();
        let keys = (0..width)
            .map(|i| format!("__fastdb_v{i}"))
            .collect::<Vec<_>>();
        let columns = keys
            .iter()
            .zip(&names)
            .map(|(key, name)| format!("{key} AS {}", quote(name)))
            .collect::<Vec<_>>()
            .join(",");
        let compound = if distinct {
            lower_set_operations(&mut definitions, &lowered, &keys, &select.body.compounds)
        } else {
            lowered.join(" UNION ALL ")
        };
        let Cmd::Stmt(Stmt::Select(mut result)) = parsed(&format!(
            "WITH {}, __fastdb_union({}) AS ({}) SELECT {columns} FROM __fastdb_union",
            definitions.join(","),
            keys.join(","),
            compound
        ))?
        else {
            unreachable!()
        };
        // Keep user CTE definitions at one shared scope, including MATERIALIZED hints.
        let generated = result.with.take().expect("generated WITH");
        result.with = Some(match select.with.clone() {
            Some(mut with) => {
                with.ctes.extend(generated.ctes);
                with
            }
            None => generated,
        });
        result.order_by = select.order_by.clone();
        for sorted in &mut result.order_by {
            let position = projection_position(&sorted.expr);
            let index = match order_base(&sorted.expr) {
                // The pinned compound resolver searches each arm in source order.
                // Result labels still come exclusively from the first arm.
                Expr::Id(name) | Expr::Name(name) => order_names.iter().find_map(|names| {
                    names
                        .iter()
                        .position(|n| n.eq_ignore_ascii_case(name.as_str()))
                }),
                _ => position.filter(|p| *p > 0 && *p <= width).map(|p| p - 1),
            }
            .ok_or_else(|| unsupported("compound ORDER BY requires an output name or position"))?;
            replace_order_base(
                &mut sorted.expr,
                expression(&format!("__fastdb_sort_encoded({})", keys[index]))?,
            );
        }
        result.limit = pagination;
        let command = Cmd::Stmt(Stmt::Select(result));
        Ok(Some(LoweredSelect {
            command,
            typed: vec![true; width],
            fetched: vec![false; width],
            names,
            consumed,
            ignore_unused: options.ignore_unused,
            explain: false,
            native_insert: false,
        }))
    }
    fn finish_select(
        &self,
        plan: LoweredSelect,
        native_insert: Option<&Stmt>,
        restricted_native_clauses: bool,
    ) -> Result<LoweredSelect> {
        let LoweredSelect {
            command: cmd,
            typed,
            fetched,
            names,
            consumed,
            ignore_unused,
            explain,
            ..
        } = plan;
        if let Some(insert) = native_insert {
            if restricted_native_clauses {
                return Err(unsupported(
                    "subqueries outside a leading-WITH INSERT SELECT source",
                ));
            }
            if explain || fetched.iter().any(|value| *value) {
                return Err(unsupported("EXPLAIN or fetched INSERT SELECT source"));
            }
            let keys = (0..typed.len())
                .map(|i| format!("__fastdb_v{i}"))
                .collect::<Vec<_>>();
            let values = keys
                .iter()
                .zip(&typed)
                .map(|(key, typed)| {
                    if *typed {
                        format!("__fastdb_sql_scalar({key})")
                    } else {
                        key.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            let source_sql = cmd.to_string();
            let Cmd::Stmt(Stmt::Select(source)) = parsed(&format!(
                "WITH __fastdb_native_source({}) AS ({}) SELECT {values} FROM __fastdb_native_source WHERE 1",
                keys.join(","), source_sql.trim().trim_end_matches(';')
            ))? else { unreachable!("generated native source") };
            let mut insert = insert.clone();
            let Stmt::Insert {
                body: InsertBody::Select(select, _),
                ..
            } = &mut insert
            else {
                unreachable!("INSERT SELECT template")
            };
            *select = source;
            return Ok(LoweredSelect {
                command: Cmd::Stmt(insert),
                typed: Vec::new(),
                fetched: Vec::new(),
                names: Vec::new(),
                consumed,
                ignore_unused: false,
                explain: false,
                native_insert: true,
            });
        }
        Ok(LoweredSelect {
            command: cmd,
            typed,
            fetched,
            names,
            consumed,
            ignore_unused,
            explain,
            native_insert: false,
        })
    }
    fn execute_lowered_select(
        &self,
        plan: LoweredSelect,
        params: &Parameters,
    ) -> Result<QueryResult> {
        self.execute_lowered_profiled(plan, params)
            .map(|profile| profile.result)
    }
    fn execute_lowered_profiled(
        &self,
        plan: LoweredSelect,
        params: &Parameters,
    ) -> Result<crate::ProfiledQuery> {
        self.execute_lowered_profiled_with_limits(plan, params, None)
    }
    fn execute_lowered_profiled_with_limits(
        &self,
        plan: LoweredSelect,
        params: &Parameters,
        limits: Option<crate::ResultLimits>,
    ) -> Result<crate::ProfiledQuery> {
        let LoweredSelect {
            command: cmd,
            typed,
            fetched,
            names,
            consumed,
            ignore_unused,
            explain,
            native_insert,
        } = plan;
        let lowered = cmd.to_string();
        let mut statement = self.prepare(&lowered)?;
        for (name, value) in params {
            let Some(index) = crate::bind_index(&statement, name) else {
                if ignore_unused || consumed.contains(name) {
                    continue;
                }
                return Err(Error::Parameter(name.clone()));
            };
            // Logical expressions encode and consume typed values during
            // lowering. Remaining placeholders belong to native SQL and need
            // raw scalar bindings, including unwrapped BLOB bytes.
            statement.bind_at(index, crate::scalar(value)?)?;
        }
        let engine_names: Vec<String> = (0..statement.num_columns())
            .map(|i| statement.get_column_name(i).into_owned())
            .collect();
        let mut rows = Vec::new();
        let mut budget = crate::budget::ResultBudget::new(
            limits,
            if explain || native_insert {
                &engine_names
            } else {
                &names
            },
        )?;
        let fetches_per_row = if native_insert || explain {
            0
        } else {
            fetched.iter().filter(|fetch| **fetch).count()
        };
        let mut reference_count = 0;
        let mut failure = None;
        let execution = crate::parser_stack(|| {
            statement.run_with_row_callback(|row| {
                let result = (|| -> Result<()> {
                    if fetches_per_row
                        > crate::links::MAX_FETCH_REFERENCES.saturating_sub(reference_count)
                    {
                        return Err(Error::Limit("fetch reference count exceeds 16384".into()));
                    }
                    reference_count += fetches_per_row;
                    let mut output = Vec::new();
                    for (i, value) in row.get_values().enumerate() {
                        if !native_insert && !explain && typed[i] {
                            output.push(match value {
                                turso_core::Value::Blob(b) => Value::decode(b)?,
                                turso_core::Value::Null => Value::Null,
                                _ => return Err(Error::Storage("invalid typed projection".into())),
                            });
                        } else {
                            output.push(crate::from_engine(value.clone()));
                        }
                    }
                    budget.row_with_fetches(
                        &output,
                        if native_insert || explain {
                            &[]
                        } else {
                            &fetched
                        },
                    )?;
                    rows.push(output);
                    Ok(())
                })();
                if let Err(error) = result {
                    failure = Some(error);
                    return Err(turso_core::LimboError::Interrupt);
                }
                Ok(())
            })
        });
        if let Some(error) = failure {
            return Err(error);
        }
        execution?;
        let mut metrics = crate::QueryMetrics::from_statement(&statement);
        if !native_insert && !explain && fetched.iter().any(|v| *v) {
            let refs = rows
                .iter()
                .flat_map(|row| {
                    row.iter()
                        .zip(&fetched)
                        .filter(|(_, fetch)| **fetch)
                        .map(|(value, _)| value.clone())
                })
                .collect::<Vec<_>>();
            let (values, fetch_metrics) =
                self.fetch_records_profiled_with_budget(&refs, Some(&mut budget))?;
            metrics.fetch_batches = fetch_metrics.batches;
            metrics.fetch_rows_read = fetch_metrics.rows_read;
            metrics.fetch_vm_steps = fetch_metrics.vm_steps;
            let mut values = values.into_iter();
            for row in &mut rows {
                for (value, fetch) in row.iter_mut().zip(&fetched) {
                    if *fetch {
                        *value = values.next().expect("matching fetch count");
                    }
                }
            }
        }
        Ok(crate::ProfiledQuery {
            metrics,
            result: QueryResult {
                columns: if explain || native_insert {
                    engine_names
                } else {
                    names
                },
                rows,
                affected: if native_insert {
                    statement.n_change()
                } else {
                    0
                },
            },
        })
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

fn expand_projection_aliases(
    expr: &mut Expr,
    columns: &[ResultColumn],
    protect_ordinals: bool,
    sources: &[Source],
    subqueries: &mut ExpressionSubqueries,
) -> Result<()> {
    let mut aliases = std::collections::BTreeMap::new();
    for column in columns {
        let ResultColumn::Expr(original, Some(alias)) = column else {
            continue;
        };
        // Closed SQL sources give declared columns precedence over projection
        // aliases. Open document sources retain their existing alias-first rule.
        let source_column = sources.iter().all(|source| source.derived.is_some())
            && sources.iter().any(|source| {
                source.derived.as_ref().is_some_and(|columns| {
                    columns
                        .iter()
                        .any(|(name, _)| name.eq_ignore_ascii_case(alias.name().as_str()))
                })
            });
        if alias.is_explicit() && !source_column {
            let value = if protect_ordinals && projection_position(original).is_some() {
                expression(&format!("coalesce({original}, NULL)"))?
            } else {
                *original.clone()
            };
            aliases
                .entry(alias.name().as_str().to_ascii_lowercase())
                .or_insert(value);
        }
    }
    let mut failure = None;
    turso_core::walk_expr_mut(expr, &mut |expr| {
        if matches!(expr, Expr::InSelect { .. }) {
            let original = expr.to_string();
            if let Expr::InSelect { lhs, .. } = expr {
                if let Err(error) =
                    expand_projection_aliases(lhs, columns, protect_ordinals, sources, subqueries)
                {
                    failure = Some(error);
                    return Ok(turso_core::WalkControl::SkipChildren);
                }
            }
            // Membership metadata describes the RHS, but its lookup key includes
            // the LHS. Keep that metadata reachable after alias substitution.
            if let Some(plan) = subqueries.get(&original).cloned() {
                subqueries.insert(expr.to_string(), plan);
            }
            return Ok(turso_core::WalkControl::SkipChildren);
        }
        if matches!(expr, Expr::FunctionCall {name,..} if name.as_str()=="__fastdb_path") {
            return Ok(turso_core::WalkControl::SkipChildren);
        }
        if matches!(expr, Expr::Id(name) if !name.quoted() && (name.as_str().eq_ignore_ascii_case("true") || name.as_str().eq_ignore_ascii_case("false")))
        {
            return Ok(turso_core::WalkControl::Continue);
        }
        if let Expr::Id(name) | Expr::Name(name) = expr {
            if let Some(value) = aliases.get(&name.as_str().to_ascii_lowercase()) {
                *expr = value.clone();
                // References inside the source expression retain source meaning,
                // even if they happen to name another projection alias.
                return Ok(turso_core::WalkControl::SkipChildren);
            }
        }
        Ok(turso_core::WalkControl::Continue)
    })?;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}

// Prepare, but do not execute, a native star query so views, generated columns,
// hidden columns and quoted names follow the pinned engine's own expansion.
fn expand_stars(
    connection: &Connection,
    scope: &Scope,
    columns: &[ResultColumn],
    from: Option<&FromClause>,
) -> Result<Vec<ResultColumn>> {
    let mut star_sources = scope.sources.iter().collect::<Vec<_>>();
    if from
        .and_then(|from| from.joins.first())
        .is_some_and(|join| {
            matches!(join.operator, JoinOperator::TypedJoin(Some(kind))
            if kind.contains(JoinType::RIGHT) && !kind.contains(JoinType::LEFT))
        })
        && star_sources.len() >= 2
    {
        // Mirror the pinned planner: RIGHT swaps the first pair, and its
        // select_star then reverses the entire joined source list.
        star_sources.swap(0, 1);
        star_sources.reverse();
    }
    let mut expanded = Vec::new();
    for column in columns {
        let sources: Vec<&Source> = match column {
            ResultColumn::Star => star_sources.clone(),
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
            let source_index = scope
                .sources
                .iter()
                .position(|candidate| std::ptr::eq(candidate, source))
                .unwrap();
            let hidden = |name: &str| {
                matches!(column, ResultColumn::Star)
                    && scope
                        .using
                        .hidden
                        .contains(&(source_index, name.to_ascii_lowercase()))
            };
            if let Some(columns) = &source.derived {
                for (position, (name, _)) in columns.iter().enumerate() {
                    if hidden(name) {
                        continue;
                    }
                    let physical = source
                        .derived_physical
                        .as_ref()
                        .map_or(name, |names| &names[position]);
                    expanded.push(ResultColumn::Expr(
                        Box::new(expression(&format!(
                            "{}.{}",
                            quote(&source.alias),
                            quote(physical)
                        ))?),
                        Some(As::As(Name::from_string(quote(name)))),
                    ));
                }
                continue;
            }
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
            let statement = connection.prepare(Cmd::Stmt(Stmt::Select(probe)).to_string())?;
            for i in 0..statement.num_columns() {
                let name = statement.get_column_name(i).into_owned();
                if hidden(&name) {
                    continue;
                }
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

// Fuse only the plain physical-document accessor produced by typed lowering.
// Other expression shapes retain the generic typed-value conversion.
fn vector_input_expression(arg: &Expr) -> Result<Expr> {
    if let Expr::FunctionCall {
        name,
        args,
        distinctness,
        filter_over,
        order_by,
        within_group,
    } = arg
    {
        if name.as_str() == "__fastdb_value"
            && args.len() == 2
            && distinctness.is_none()
            && filter_over.over_clause.is_none()
            && filter_over.filter_clause.is_none()
            && order_by.is_empty()
            && within_group.is_empty()
        {
            let mut fused = arg.clone();
            if let Expr::FunctionCall { name, .. } = &mut fused {
                *name = Name::exact("__fastdb_vector_field".into());
            }
            return Ok(fused);
        }
    }
    expression(&format!("__fastdb_vector_input({arg})"))
}

#[cfg(test)]
mod lowering_tests {
    #[test]
    fn pagination_text_adapter_matches_pinned_conversion() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        let empty = crate::Parameters::new();
        c.execute("CREATE TABLE pagination_probe(n INTEGER)", &empty)
            .unwrap();
        c.execute("INSERT INTO pagination_probe VALUES(1),(2),(3)", &empty)
            .unwrap();
        for sign in ["", "+", "-"] {
            for mantissa in ["", ".", "0", "1", "1.", ".5", "1.5", "01", "1..0"] {
                for exponent in ["", "e0", "E+1", "e-1", "e", "e+", "e1x"] {
                    for whitespace in ["", " "] {
                        let text = format!("{whitespace}{sign}{mantissa}{exponent}{whitespace}");
                        let value = crate::Value::String(text.clone());
                        let converted = super::pagination_integer(&value);
                        let params = crate::Parameters::from([("$value".into(), value)]);
                        let sql = "SELECT n FROM pagination_probe ORDER BY n LIMIT $value";
                        match c.execute(sql, &params) {
                            Ok(actual) => {
                                let integer =
                                    converted.unwrap_or_else(|| panic!("native accepted {text:?}"));
                                let expected = c
                                    .execute(&sql.replace("$value", &integer.to_string()), &empty)
                                    .unwrap();
                                assert_eq!(actual.rows, expected.rows, "{text:?}: {integer}");
                                // MustBeInt acceptance was established above. Numeric
                                // CAST exposes the exact converted value, including
                                // exponent text that a direct INTEGER cast truncates.
                                let numeric = c
                                    .execute(
                                        "SELECT CAST(CAST($value AS NUMERIC) AS INTEGER)",
                                        &params,
                                    )
                                    .unwrap();
                                assert_eq!(
                                    numeric.rows,
                                    vec![vec![crate::Value::Integer(integer)]],
                                    "exact conversion: {text:?}"
                                );
                            }
                            Err(error) => {
                                assert!(
                                    error.to_string().contains("datatype mismatch"),
                                    "{text:?}: {error}"
                                );
                                assert_eq!(converted, None, "native rejected {text:?}");
                            }
                        }
                    }
                }
            }
        }
    }
    use super::*;
    #[test]
    fn wide_duplicate_names_preserve_public_names_and_avoid_collisions() {
        let mut names = vec![
            "X".to_owned(),
            "x".to_owned(),
            "__fastdb_derived_column_1".to_owned(),
            "__FASTDB_DERIVED_COLUMN_1_".to_owned(),
        ];
        names.extend(std::iter::repeat_n("x".to_owned(), 16_384));
        let physical = derived_physical_names(&names);
        assert_eq!(physical.len(), names.len());
        assert_eq!(physical[0], "X");
        assert_eq!(physical[1], "__fastdb_derived_column_1__");
        assert_eq!(physical[2], names[2]);
        assert_eq!(physical[3], names[3]);
        let unique = physical
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), physical.len());
    }

    #[test]
    fn lowering_preserves_typed_outputs_and_defers_native_insert_execution() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        let empty = Parameters::new();
        c.execute("CREATE TABLE docs", &empty).unwrap();
        c.execute("CREATE TABLE copied(data BLOB)", &empty).unwrap();
        let params = Parameters::from([("$flag".into(), Value::Boolean(true))]);
        c.execute(
            "INSERT INTO docs (id,flag,data) VALUES (docs:a,$flag,X'31')",
            &params,
        )
        .unwrap();
        let sql = "SELECT flag,data,id FROM docs WHERE flag=$flag";
        let expanded = expand_paths(&expand_records(sql).unwrap()).unwrap();
        let plan = c
            .lower_collection_select(sql, &expanded, &params, SelectOptions::default())
            .unwrap()
            .unwrap();
        assert_eq!(plan.names, vec!["flag", "data", "id"]);
        assert_eq!(plan.typed, vec![true, true, true]);
        let result = c.execute_lowered_select(plan, &params).unwrap();
        assert_eq!(
            result.rows,
            vec![vec![
                Value::Boolean(true),
                Value::Binary(vec![49]),
                Value::Record(crate::Record {
                    table: "docs".into(),
                    key: crate::Key::String("a".into())
                })
            ]]
        );
        let source = "SELECT data FROM docs WHERE flag=$flag";
        let expanded = expand_paths(&expand_records(source).unwrap()).unwrap();
        let Cmd::Stmt(insert) =
            parsed("INSERT INTO copied SELECT data FROM docs WHERE flag=$flag RETURNING hex(data)")
                .unwrap()
        else {
            unreachable!();
        };
        let plan = c
            .lower_collection_select(
                source,
                &expanded,
                &params,
                SelectOptions {
                    trusted: true,
                    positional: true,
                    native_insert: Some(&insert),
                    ..Default::default()
                },
            )
            .unwrap()
            .unwrap();
        assert!(c
            .execute("SELECT * FROM copied", &empty)
            .unwrap()
            .rows
            .is_empty());
        let result = c.execute_lowered_select(plan, &params).unwrap();
        assert_eq!(result.affected, 1);
        assert_eq!(result.rows, vec![vec![Value::String("31".into())]]);
        assert_eq!(
            c.execute("SELECT data FROM copied", &empty).unwrap().rows,
            vec![vec![Value::Binary(vec![49])]]
        );
    }
}
