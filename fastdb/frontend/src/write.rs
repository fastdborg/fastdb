//! SQL-shaped writes share the document validation and index-maintenance path.
use crate::{
    select::{expand_paths, expand_records, parsed},
    Connection, Document, Error, Parameters, QueryResult, Result, Value,
};
use turso_parser::ast::*;

fn unsupported(message: &str) -> Error {
    Error::Unsupported(message.into())
}
pub(crate) fn validate_returning(columns: &[ResultColumn]) -> Result<()> {
    for column in columns {
        if let ResultColumn::Expr(expr, _) = column {
            safe_value_expression(expr)?;
        }
    }
    Ok(())
}
fn id(doc: &Document) -> Result<&crate::Record> {
    match doc.get("id") {
        Some(Value::Record(id)) => Ok(id),
        _ => Err(Error::Storage("stored document has no typed id".into())),
    }
}
fn parameter<'a>(expr: &Expr, params: &'a Parameters) -> Result<Option<&'a Value>> {
    if let Expr::Variable(var) = expr {
        let name = var
            .name
            .as_ref()
            .map_or_else(|| format!("?{}", var.index), |s| s.to_string());
        return params.get(&name).map(Some).ok_or(Error::Parameter(name));
    }
    Ok(None)
}
fn safe_value_expression(expr: &Expr) -> Result<()> {
    match expr {
        Expr::Literal(_)
        | Expr::Variable(_)
        | Expr::Id(_)
        | Expr::Name(_)
        | Expr::Qualified(..)
        | Expr::DoublyQualified(..) => Ok(()),
        Expr::Unary(_, e)
        | Expr::Cast { expr: e, .. }
        | Expr::Collate(e, _)
        | Expr::IsNull(e)
        | Expr::NotNull(e) => safe_value_expression(e),
        Expr::Binary(a, _, b) => {
            safe_value_expression(a)?;
            safe_value_expression(b)
        }
        Expr::Between {
            lhs, start, end, ..
        } => {
            safe_value_expression(lhs)?;
            safe_value_expression(start)?;
            safe_value_expression(end)
        }
        Expr::InList { lhs, rhs, .. } => {
            safe_value_expression(lhs)?;
            for e in rhs {
                safe_value_expression(e)?;
            }
            Ok(())
        }
        Expr::Like {
            lhs, rhs, escape, ..
        } => {
            safe_value_expression(lhs)?;
            safe_value_expression(rhs)?;
            if let Some(e) = escape {
                safe_value_expression(e)?;
            }
            Ok(())
        }
        Expr::Case {
            base,
            when_then_pairs,
            else_expr,
        } => {
            if let Some(base) = base {
                safe_value_expression(base)?;
            }
            for (condition, value) in when_then_pairs {
                safe_value_expression(condition)?;
                safe_value_expression(value)?;
            }
            if let Some(value) = else_expr {
                safe_value_expression(value)?;
            }
            Ok(())
        }
        Expr::Parenthesized(es) => {
            for e in es {
                safe_value_expression(e)?;
            }
            Ok(())
        }
        Expr::FunctionCall {
            name,
            args,
            filter_over,
            order_by,
            within_group,
            ..
        } => {
            if filter_over.filter_clause.is_some()
                || filter_over.over_clause.is_some()
                || !order_by.is_empty()
                || !within_group.is_empty()
            {
                return Err(unsupported("aggregate/window VALUES expressions"));
            }
            let function = name.as_str().to_ascii_lowercase();
            if function == "__fastdb_fetch" {
                return Err(unsupported("record::fetch in document write expression"));
            }
            if matches!(
                function.as_str(),
                "avg"
                    | "count"
                    | "group_concat"
                    | "string_agg"
                    | "sum"
                    | "total"
                    | "json_group_array"
                    | "jsonb_group_array"
                    | "json_group_object"
                    | "jsonb_group_object"
                    | "array_agg"
                    | "mode"
                    | "percentile_cont"
                    | "percentile_disc"
            ) || (matches!(function.as_str(), "min" | "max") && args.len() <= 1)
            {
                return Err(unsupported("aggregate document write expressions"));
            }
            if name.as_str().eq_ignore_ascii_case("load_extension") {
                return Err(unsupported("extension loading in document writes"));
            }
            for e in args {
                safe_value_expression(e)?;
            }
            Ok(())
        }
        _ => Err(unsupported("this collection VALUES expression")),
    }
}
// Candidate SELECT lowering owns subquery validation and typed result handling.
// Validate only the surrounding scalar assignment here; VALUES/RETURNING keep
// their stricter validator. Never execute this validation-only expression.
fn safe_candidate_assignment(expr: &Expr) -> Result<()> {
    let mut outer = expr.clone();
    turso_core::walk_expr_mut(&mut outer, &mut |expr| {
        match expr {
            Expr::Subquery(_) | Expr::Exists(_) => {
                *expr = Expr::Literal(Literal::Null);
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            Expr::InSelect { lhs, .. } => {
                *expr = Expr::Parenthesized(vec![lhs.clone()]);
            }
            _ => {}
        }
        Ok(turso_core::WalkControl::Continue)
    })?;
    safe_value_expression(&outer)
}

fn insert_clause_subqueries(statement: &Stmt) -> Result<bool> {
    let Stmt::Insert {
        body: InsertBody::Select(_, upsert),
        returning,
        ..
    } = statement
    else {
        return Ok(false);
    };
    let mut found = false;
    let mut check = |expr: &Expr| -> Result<()> {
        turso_core::walk_expr_mut(&mut expr.clone(), &mut |expr| {
            if matches!(
                expr,
                Expr::Subquery(_) | Expr::Exists(_) | Expr::InSelect { .. } | Expr::InTable { .. }
            ) {
                found = true;
                return Ok(turso_core::WalkControl::SkipChildren);
            }
            Ok(turso_core::WalkControl::Continue)
        })?;
        Ok(())
    };
    for column in returning {
        if let ResultColumn::Expr(expr, _) = column {
            check(expr)?;
        }
    }
    let mut current = upsert.as_deref();
    while let Some(clause) = current {
        if let Some(index) = &clause.index {
            for target in &index.targets {
                check(&target.expr)?;
            }
            if let Some(expr) = &index.where_clause {
                check(expr)?;
            }
        }
        if let UpsertDo::Set { sets, where_clause } = &clause.do_clause {
            for set in sets {
                check(&set.expr)?;
            }
            if let Some(expr) = where_clause {
                check(expr)?;
            }
        }
        current = clause.next.as_deref();
    }
    Ok(found)
}
impl Connection {
    pub(crate) fn object_returning(
        &self,
        table: &str,
        projection: Option<String>,
        documents: Vec<Document>,
        params: &Parameters,
    ) -> Result<QueryResult> {
        let Some(projection) = projection else {
            return Ok(QueryResult::command(documents.len() as i64));
        };
        crate::guard::internal_names(&projection)?;
        let Cmd::Stmt(Stmt::Select(select)) = parsed(&expand_paths(&expand_records(&format!(
            "SELECT {projection}"
        ))?)?)?
        else {
            return Err(unsupported("RETURNING projection"));
        };
        if select.with.is_some()
            || !select.body.compounds.is_empty()
            || !select.order_by.is_empty()
            || select.limit.is_some()
        {
            return Err(unsupported("RETURNING clauses"));
        }
        let OneSelect::Select {
            columns,
            from: None,
            where_clause: None,
            group_by: None,
            window_clause,
            distinctness: None,
        } = select.body.select
        else {
            return Err(unsupported("RETURNING projection list only"));
        };
        if !window_clause.is_empty() {
            return Err(unsupported("RETURNING windows"));
        }
        validate_returning(&columns)?;
        self.returning_rows(
            &QualifiedName::single(Name::from_string(crate::quote(table))),
            &columns,
            documents,
            params,
        )
    }
    fn relational_insert_select(
        &self,
        sql: &str,
        statement: &Stmt,
        params: &Parameters,
        leading_with: bool,
    ) -> Result<Option<QueryResult>> {
        let Stmt::Insert {
            with: None,
            body: InsertBody::Select(select, _),
            ..
        } = statement
        else {
            return Ok(None);
        };
        if matches!(select.body.select, OneSelect::Values(_)) {
            return Ok(None);
        }
        // Source names are checked by logical lowering. Guard the target and
        // remaining native clauses with the existing native SQL boundary.
        for token in crate::guard::tokens(sql)? {
            if matches!(
                token.kind,
                fastql_parser::Kind::Word
                    | fastql_parser::Kind::Identifier
                    | fastql_parser::Kind::String
            ) && token.text.to_ascii_lowercase().starts_with("__fastdb_")
            {
                return Err(unsupported("managed names in INSERT SELECT"));
            }
        }
        let mut guarded = statement.clone();
        let Stmt::Insert {
            body: InsertBody::Select(source, _),
            ..
        } = &mut guarded
        else {
            unreachable!()
        };
        let Cmd::Stmt(Stmt::Select(empty)) = parsed("SELECT NULL")? else {
            unreachable!()
        };
        *source = empty;
        self.guard_native_sql(&guarded.to_string())?;
        // Moving a leading WITH into the source cannot preserve CTE scope in
        // subqueries in UPSERT/RETURNING. Reject those on managed-source routes;
        // native-only statements still fall back to their original SQL.
        let restricted_clauses = leading_with && insert_clause_subqueries(&guarded)?;
        self.native_insert_source(
            &Stmt::Select(select.clone()).to_string(),
            params,
            statement,
            restricted_clauses,
        )
    }
    pub(crate) fn collection_write(
        &self,
        sql: &str,
        params: &Parameters,
    ) -> Result<Option<QueryResult>> {
        let normalized = crate::update::normalize(sql)?;
        let expanded = expand_paths(&expand_records(
            normalized.as_ref().map_or(sql, |n| n.sql.as_str()),
        )?)?;
        let Ok(Cmd::Stmt(mut statement)) = parsed(&expanded) else {
            return Ok(None);
        };
        let leading_with = if let Stmt::Insert {
            with,
            body: InsertBody::Select(select, _),
            ..
        } = &mut statement
        {
            if with.is_some() && select.with.is_none() {
                select.with = with.take();
                true
            } else {
                false
            }
        } else {
            false
        };
        let table = match &statement {
            Stmt::Insert { tbl_name, .. } | Stmt::Delete { tbl_name, .. } => tbl_name,
            Stmt::Update(update) => &update.tbl_name,
            _ => return Ok(None),
        };
        if table
            .name
            .as_str()
            .to_ascii_lowercase()
            .starts_with("__fastdb_")
        {
            return Err(unsupported("managed storage writes"));
        }
        match self.catalog(table.name.as_str()) {
            Ok(_) => {}
            Err(Error::NotFound(_)) => {
                return self.relational_insert_select(sql, &statement, params, leading_with)
            }
            Err(e) => return Err(e),
        };
        if table
            .db_name
            .as_ref()
            .is_some_and(|n| !n.as_str().eq_ignore_ascii_case("main"))
        {
            return Err(unsupported("attached collection writes"));
        }
        crate::guard::internal_names(sql)?;
        self.atomic(|| match statement {
            Stmt::Insert {
                with,
                or_conflict,
                tbl_name,
                columns,
                body,
                returning,
            } => {
                if with.is_some() || or_conflict.is_some() {
                    return Err(unsupported(
                        "collection INSERT WITH/OR CONFLICT; use document UPSERT",
                    ));
                }
                validate_returning(&returning)?;
                if columns.is_empty() {
                    return Err(Error::Validation(
                        "collection SQL INSERT requires a column list".into(),
                    ));
                }
                let fields: Vec<String> =
                    columns.into_iter().map(|n| n.as_str().to_owned()).collect();
                for (i, name) in fields.iter().enumerate() {
                    if fields[..i].contains(name) {
                        return Err(Error::Validation("duplicate INSERT field".into()));
                    }
                }
                let InsertBody::Select(select, upsert) = body else {
                    return Err(unsupported("collection DEFAULT VALUES"));
                };
                if upsert.is_some() {
                    return Err(unsupported("collection ON CONFLICT; use document UPSERT"));
                }
                let values = if let (OneSelect::Values(rows), None, true) = (
                    &select.body.select,
                    &select.with,
                    select.body.compounds.is_empty(),
                ) {
                    if !select.body.compounds.is_empty()
                        || !select.order_by.is_empty()
                        || select.limit.is_some()
                    {
                        return Err(unsupported("compound or modified VALUES source"));
                    }
                    rows.iter()
                        .map(|row| {
                            if row.len() != fields.len() {
                                return Err(Error::Validation(
                                    "INSERT field/value count mismatch".into(),
                                ));
                            }
                            row.iter()
                                .map(|expr| self.write_value(expr, params))
                                .collect::<Result<Vec<_>>>()
                        })
                        .collect::<Result<Vec<_>>>()?
                } else {
                    let selected = self.insert_select(&Stmt::Select(select).to_string(), params)?;
                    if selected.columns.len() != fields.len() {
                        return Err(Error::Validation(
                            "INSERT field/value count mismatch".into(),
                        ));
                    }
                    selected.rows
                };
                // Materialize the source before mutation, including self-inserts.
                let mut documents = Vec::new();
                for row in values {
                    let doc = fields.iter().cloned().zip(row).collect();
                    documents.push(self.insert(tbl_name.name.as_str(), doc)?);
                }
                Ok(Some(self.returning_rows(
                    &tbl_name, &returning, documents, params,
                )?))
            }
            Stmt::Update(update) => {
                if update.or_conflict.is_some()
                    || update.from.is_some()
                    || update.indexed.is_some()
                    || !update.order_by.is_empty()
                    || update.limit.is_some()
                {
                    return Err(unsupported("this collection UPDATE clause"));
                }
                validate_returning(&update.returning)?;
                let mut fields = Vec::new();
                let mut exprs = Vec::new();
                for set in update.sets {
                    if set.col_names.len() != 1 {
                        return Err(unsupported("tuple collection assignments"));
                    }
                    let field = set.col_names[0].as_str().to_owned();
                    fields.push(field);
                    exprs.push(*set.expr);
                }
                let paths = normalized.as_ref().map_or_else(
                    || fields.iter().map(|f| vec![f.clone()]).collect(),
                    |n| n.paths.clone(),
                );
                crate::update::validate_targets(&paths)?;
                let unset = normalized.as_ref().is_some_and(|n| n.unset);
                let rows = self.write_candidates(
                    &update.tbl_name,
                    update.with,
                    update.where_clause,
                    &exprs,
                    params,
                )?;
                let mut documents = Vec::new();
                for row in rows {
                    let Value::Object(original) = &row[0] else {
                        return Err(Error::Storage("invalid update candidate".into()));
                    };
                    let mut document = original.clone();
                    for (path, value) in paths.iter().zip(row[1..].iter()) {
                        crate::update::apply(
                            &mut document,
                            path,
                            if unset { None } else { Some(value.clone()) },
                        )?;
                    }
                    let collection = self.catalog(&id(original)?.table)?;
                    self.replace_document(&collection, &document)?;
                    documents.push(document);
                }
                Ok(Some(self.returning_rows(
                    &update.tbl_name,
                    &update.returning,
                    documents,
                    params,
                )?))
            }
            Stmt::Delete {
                with,
                tbl_name,
                indexed,
                where_clause,
                returning,
                order_by,
                limit,
            } => {
                if indexed.is_some() || !order_by.is_empty() || limit.is_some() {
                    return Err(unsupported("this collection DELETE clause"));
                }
                validate_returning(&returning)?;
                let rows = self.write_candidates(&tbl_name, with, where_clause, &[], params)?;
                let mut documents = Vec::new();
                for row in rows {
                    let Value::Object(doc) = &row[0] else {
                        return Err(Error::Storage("invalid delete candidate".into()));
                    };
                    documents
                        .push(self.delete(id(doc)?)?.ok_or_else(|| {
                            Error::Storage("delete candidate disappeared".into())
                        })?);
                }
                Ok(Some(self.returning_rows(
                    &tbl_name, &returning, documents, params,
                )?))
            }
            _ => unreachable!("write statement dispatched above"),
        })
    }
    fn write_value(&self, expr: &Expr, params: &Parameters) -> Result<Value> {
        if let Some(value) = parameter(expr, params)? {
            value.validate()?;
            return Ok(value.clone());
        }
        safe_value_expression(expr)?;
        let rows = self
            .collection_select_internal(&format!("SELECT {expr}"), params, true)?
            .ok_or_else(|| Error::Storage("VALUES expression lowering failed".into()))?
            .rows;
        rows.into_iter()
            .next()
            .and_then(|row| row.into_iter().next())
            .ok_or_else(|| Error::Storage("VALUES expression returned no value".into()))
    }

    fn write_candidates(
        &self,
        table: &QualifiedName,
        with: Option<With>,
        predicate: Option<Box<Expr>>,
        assignments: &[Expr],
        params: &Parameters,
    ) -> Result<Vec<Vec<Value>>> {
        for expr in assignments {
            safe_candidate_assignment(expr)?;
        }
        let mut columns = vec![ResultColumn::Star];
        for (i, expr) in assignments.iter().enumerate() {
            let expr = if parameter(expr, params)?.is_some() {
                Expr::Literal(Literal::Null)
            } else {
                expr.clone()
            };
            columns.push(ResultColumn::Expr(
                Box::new(expr),
                Some(As::As(Name::exact(format!("__fastdb_set_{i}")))),
            ));
        }
        let mut with = with;
        // The pinned write planner exposes its target table before replanning
        // CTE bodies. A same-named FROM there binds the physical target rather
        // than the CTE. Preserve that binding in the candidate SELECT.
        if let Some(with) = with.as_mut().filter(|with| !with.recursive) {
            let exposed = table.alias.as_ref().unwrap_or(&table.name);
            if with
                .ctes
                .iter()
                .any(|cte| cte.tbl_name.as_str().eq_ignore_ascii_case(exposed.as_str()))
            {
                fn bind_target(select: &mut Select, table: &QualifiedName, exposed: &Name) {
                    if select.with.is_some() {
                        return;
                    }
                    for body in std::iter::once(&mut select.body.select)
                        .chain(select.body.compounds.iter_mut().map(|arm| &mut arm.select))
                    {
                        if let OneSelect::Select {
                            from: Some(from), ..
                        } = body
                        {
                            for source in std::iter::once(&mut from.select)
                                .chain(from.joins.iter_mut().map(|join| &mut join.table))
                            {
                                match source.as_mut() {
                                    SelectTable::Select(inner, _) => {
                                        bind_target(inner, table, exposed)
                                    }
                                    SelectTable::Table(name, alias, _)
                                        if name.db_name.is_none()
                                            && name
                                                .name
                                                .as_str()
                                                .eq_ignore_ascii_case(exposed.as_str()) =>
                                    {
                                        name.db_name = Some(Name::exact("main".into()));
                                        name.name = table.name.clone();
                                        if alias.is_none() {
                                            *alias = Some(As::As(exposed.clone()));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                for cte in &mut with.ctes {
                    bind_target(&mut cte.select, table, exposed);
                }
            }
        }
        let mut target = table.clone();
        // UPDATE/DELETE target names refer to actual tables, even if a CTE
        // has the same name. Preserve that rule in the candidate SELECT.
        if with.is_some() && target.db_name.is_none() {
            target.db_name = Some(Name::exact("main".into()));
        }
        let alias = target.alias.take().map(As::As);
        let select = Select {
            with,
            body: SelectBody {
                select: OneSelect::Select {
                    distinctness: None,
                    columns,
                    from: Some(FromClause {
                        select: Box::new(SelectTable::Table(target, alias, None)),
                        joins: Vec::new(),
                    }),
                    where_clause: predicate,
                    group_by: None,
                    window_clause: Vec::new(),
                },
                compounds: Vec::new(),
            },
            order_by: Vec::new(),
            limit: None,
        };
        let mut result = self
            .collection_select_internal(&Stmt::Select(select).to_string(), params, true)?
            .ok_or_else(|| Error::Storage("write target was not a collection".into()))?;
        for row in &mut result.rows {
            for (i, expr) in assignments.iter().enumerate() {
                if let Some(value) = parameter(expr, params)? {
                    value.validate()?;
                    row[i + 1] = value.clone();
                }
            }
        }
        Ok(result.rows)
    }
}
