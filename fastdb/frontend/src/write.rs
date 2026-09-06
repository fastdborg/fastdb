//! SQL-shaped writes share the document validation and index-maintenance path.
use crate::{
    select::{expand_records, parsed},
    Connection, Document, Error, Parameters, QueryResult, Result, Value,
};
use turso_parser::ast::*;

fn unsupported(message: &str) -> Error {
    Error::Unsupported(message.into())
}
fn returning_star(columns: &[ResultColumn]) -> Result<bool> {
    match columns {
        [] => Ok(false),
        [ResultColumn::Star] => Ok(true),
        _ => Err(unsupported(
            "collection writes currently support RETURNING *",
        )),
    }
}
fn result(documents: Vec<Document>, returning: bool) -> QueryResult {
    let affected = documents.len() as i64;
    if returning {
        QueryResult::documents(documents, affected)
    } else {
        QueryResult::command(affected)
    }
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
        Expr::Literal(_) | Expr::Variable(_) => Ok(()),
        Expr::Unary(_, e) | Expr::Cast { expr: e, .. } | Expr::Collate(e, _) => {
            safe_value_expression(e)
        }
        Expr::Binary(a, _, b) => {
            safe_value_expression(a)?;
            safe_value_expression(b)
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
impl Connection {
    pub(crate) fn collection_write(
        &self,
        sql: &str,
        params: &Parameters,
    ) -> Result<Option<QueryResult>> {
        let normalized = crate::update::normalize(sql)?;
        let expanded = expand_records(normalized.as_ref().map_or(sql, |n| n.sql.as_str()))?;
        let Ok(Cmd::Stmt(statement)) = parsed(&expanded) else {
            return Ok(None);
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
            Err(Error::NotFound(_)) => return Ok(None),
            Err(e) => return Err(e),
        };
        if table
            .db_name
            .as_ref()
            .is_some_and(|n| !n.as_str().eq_ignore_ascii_case("main"))
        {
            return Err(unsupported("attached collection writes"));
        }
        for token in fastql_parser::tokenize(sql)? {
            if matches!(
                token.kind,
                fastql_parser::Kind::Word | fastql_parser::Kind::Identifier
            ) && token.text.to_ascii_lowercase().starts_with("__fastdb_")
            {
                return Err(unsupported("managed names in document writes"));
            }
        }
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
                let returning = returning_star(&returning)?;
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
                let OneSelect::Values(rows) = select.body.select else {
                    return Err(unsupported("collection INSERT SELECT is not implemented"));
                };
                let mut documents = Vec::new();
                for row in rows {
                    if row.len() != fields.len() {
                        return Err(Error::Validation(
                            "INSERT field/value count mismatch".into(),
                        ));
                    }
                    let mut doc = Document::new();
                    for (field, expr) in fields.iter().zip(row) {
                        doc.insert(field.clone(), self.write_value(&expr, params)?);
                    }
                    documents.push(self.insert(tbl_name.name.as_str(), doc)?);
                }
                Ok(Some(result(documents, returning)))
            }
            Stmt::Update(update) => {
                if update.with.is_some()
                    || update.or_conflict.is_some()
                    || update.from.is_some()
                    || update.indexed.is_some()
                    || !update.order_by.is_empty()
                    || update.limit.is_some()
                {
                    return Err(unsupported("this collection UPDATE clause"));
                }
                let returning = returning_star(&update.returning)?;
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
                let rows =
                    self.write_candidates(&update.tbl_name, update.where_clause, &exprs, params)?;
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
                Ok(Some(result(documents, returning)))
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
                if with.is_some() || indexed.is_some() || !order_by.is_empty() || limit.is_some() {
                    return Err(unsupported("this collection DELETE clause"));
                }
                let returning = returning_star(&returning)?;
                let rows = self.write_candidates(&tbl_name, where_clause, &[], params)?;
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
                Ok(Some(result(documents, returning)))
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
        let mut statement = self.engine.prepare(format!("SELECT {expr}"))?;
        bind_used(&mut statement, params)?;
        let rows = statement.run_collect_rows()?;
        let value = rows
            .into_iter()
            .next()
            .and_then(|row| row.into_iter().next())
            .ok_or_else(|| Error::Storage("VALUES expression returned no value".into()))?;
        if matches!(expr,Expr::FunctionCall{name,..} if name.as_str()=="__fastdb_record_value") {
            if let turso_core::Value::Blob(bytes) = value {
                return Value::decode(&bytes);
            }
            return Err(Error::Storage("record constructor result".into()));
        }
        let value = crate::from_engine(value);
        value.validate()?;
        Ok(value)
    }
    fn write_candidates(
        &self,
        table: &QualifiedName,
        predicate: Option<Box<Expr>>,
        assignments: &[Expr],
        params: &Parameters,
    ) -> Result<Vec<Vec<Value>>> {
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
        let mut target = table.clone();
        let alias = target.alias.take().map(As::As);
        let select = Select {
            with: None,
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
fn bind_used(statement: &mut turso_core::Statement, params: &Parameters) -> Result<()> {
    for (name, value) in params {
        if let Some(index) = crate::bind_index(statement, name) {
            statement.bind_at(index, crate::scalar(value)?)?;
        }
    }
    Ok(())
}
