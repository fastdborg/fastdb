//! Deterministic field CHECKs evaluate the candidate, never the stored row.
use crate::{Collection, Connection, Document, Error, Field, Result, Value};
use std::num::{NonZeroU32, NonZeroUsize};
use turso_core::Value as EngineValue;
use turso_parser::ast::*;

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn parse_check(sql: &str) -> Result<Expr> {
    let Cmd::Stmt(Stmt::Select(select)) = crate::select::parsed(&format!("SELECT ({sql})"))? else {
        return Err(invalid("CHECK must be an expression"));
    };
    if select.with.is_some()
        || !select.body.compounds.is_empty()
        || !select.order_by.is_empty()
        || select.limit.is_some()
    {
        return Err(invalid("CHECK must be an expression"));
    }
    let OneSelect::Select {
        mut columns,
        from: None,
        where_clause: None,
        group_by: None,
        window_clause,
        ..
    } = select.body.select
    else {
        return Err(invalid("CHECK cannot read the database"));
    };
    if columns.len() != 1 || !window_clause.is_empty() {
        return Err(invalid("CHECK must be one expression"));
    }
    let ResultColumn::Expr(expr, _) = columns.remove(0) else {
        return Err(invalid("CHECK must be an expression"));
    };
    Ok(*expr)
}
fn field_path(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::Parenthesized(es) if es.len() == 1 => field_path(&es[0]),
        Expr::Id(n) | Expr::Name(n) => Some(vec![n.as_str().into()]),
        Expr::Qualified(a, b) => Some(vec![a.as_str().into(), b.as_str().into()]),
        Expr::DoublyQualified(a, b, c) => Some(vec![
            a.as_str().into(),
            b.as_str().into(),
            c.as_str().into(),
        ]),
        Expr::FieldAccess { base, field, .. } => {
            let mut path = field_path(base)?;
            path.push(field.as_str().into());
            Some(path)
        }
        _ => None,
    }
}
fn bind_field(
    expr: &mut Expr,
    doc: Option<&Document>,
    bindings: &mut Vec<EngineValue>,
    typed: bool,
) -> Result<bool> {
    if let Some(path) = field_path(expr) {
        let value = match doc {
            Some(doc) => match crate::path_value(doc, &path) {
                Ok(v) => v,
                Err(Error::Validation(_)) => None,
                Err(e) => return Err(e),
            },
            None => None,
        };
        let value = value.unwrap_or(&Value::Null);
        bindings.push(if typed {
            EngineValue::Blob(value.encode()?)
        } else {
            crate::index_scalar(value)?
        });
        let index =
            u32::try_from(bindings.len()).map_err(|_| invalid("too many CHECK references"))?;
        *expr = Expr::Variable(Variable::indexed(
            NonZeroU32::new(index).expect("one-based binding"),
        ));
        return Ok(true);
    }
    Ok(false)
}
fn lower(expr: &mut Expr, doc: Option<&Document>, bindings: &mut Vec<EngineValue>) -> Result<()> {
    if bind_field(expr, doc, bindings, false)? {
        return Ok(());
    }
    match expr {
        Expr::Literal(Literal::CurrentDate | Literal::CurrentTime | Literal::CurrentTimestamp) => {
            return Err(invalid("CHECK cannot depend on current time"))
        }
        Expr::Literal(_) => {}
        Expr::Variable(_) => return Err(invalid("CHECK cannot contain bound parameters")),
        Expr::Binary(a, op, b) => {
            if matches!(
                op,
                Operator::Less | Operator::LessEquals | Operator::Greater | Operator::GreaterEquals
            ) && field_path(a).is_some()
                && field_path(b).is_some()
            {
                bind_field(a, doc, bindings, true)?;
                bind_field(b, doc, bindings, true)?;
                *expr = parse_check(&format!("__fastdb_compare({a},{b}) {op} 0"))?;
                return Ok(());
            }
            lower(a, doc, bindings)?;
            lower(b, doc, bindings)?;
        }
        Expr::Unary(_, e) | Expr::IsNull(e) | Expr::NotNull(e) => lower(e, doc, bindings)?,
        Expr::Cast { expr: e, type_name } => {
            let Some(t) = type_name else {
                return Err(invalid("CHECK CAST requires a built-in scalar type"));
            };
            if t.size.is_some()
                || t.array_dimensions != 0
                || !matches!(
                    t.name.to_ascii_lowercase().as_str(),
                    "integer" | "int" | "real" | "float" | "double" | "numeric" | "text" | "blob"
                )
            {
                return Err(invalid(
                    "CHECK CAST requires an unsized built-in scalar type",
                ));
            }
            lower(e, doc, bindings)?;
        }
        Expr::Collate(e, name) => {
            if !matches!(
                name.as_str().to_ascii_lowercase().as_str(),
                "binary" | "nocase" | "rtrim"
            ) {
                return Err(invalid("CHECK requires a built-in collation"));
            }
            lower(e, doc, bindings)?;
        }
        Expr::Between {
            lhs,
            start,
            end,
            not,
        } => {
            if field_path(lhs).is_some() && field_path(start).is_some() && field_path(end).is_some()
            {
                bind_field(lhs, doc, bindings, true)?;
                bind_field(start, doc, bindings, true)?;
                bind_field(end, doc, bindings, true)?;
                let negate = if *not { "NOT " } else { "" };
                *expr = parse_check(&format!("{negate}__fastdb_between({lhs},{start},{end})"))?;
                return Ok(());
            }
            lower(lhs, doc, bindings)?;
            lower(start, doc, bindings)?;
            lower(end, doc, bindings)?;
        }
        Expr::Like {
            lhs,
            rhs,
            escape,
            op,
            ..
        } => {
            if *op != LikeOperator::Glob {
                return Err(invalid("CHECK supports fixed GLOB matching; LIKE/REGEXP/MATCH eligibility is not defined"));
            }
            lower(lhs, doc, bindings)?;
            lower(rhs, doc, bindings)?;
            if let Some(e) = escape {
                lower(e, doc, bindings)?;
            }
        }
        Expr::InList { lhs, rhs, .. } => {
            lower(lhs, doc, bindings)?;
            for e in rhs {
                lower(e, doc, bindings)?;
            }
        }
        Expr::Parenthesized(es) => {
            for e in es {
                lower(e, doc, bindings)?;
            }
        }
        Expr::Case {
            base,
            when_then_pairs,
            else_expr,
        } => {
            if let Some(e) = base {
                lower(e, doc, bindings)?;
            }
            for (a, b) in when_then_pairs {
                lower(a, doc, bindings)?;
                lower(b, doc, bindings)?;
            }
            if let Some(e) = else_expr {
                lower(e, doc, bindings)?;
            }
        }
        Expr::FunctionCall {
            name,
            args,
            distinctness,
            filter_over,
            order_by,
            within_group,
        } => {
            let name = name.as_str().to_ascii_lowercase();
            let eligible = matches!(
                name.as_str(),
                "length"
                    | "lower"
                    | "upper"
                    | "trim"
                    | "ltrim"
                    | "rtrim"
                    | "substr"
                    | "substring"
                    | "abs"
                    | "round"
                    | "coalesce"
                    | "ifnull"
                    | "nullif"
                    | "typeof"
                    | "unicode"
                    | "instr"
                    | "replace"
            ) || (matches!(name.as_str(), "min" | "max") && args.len() >= 2);
            if !eligible
                || distinctness.is_some()
                || filter_over.filter_clause.is_some()
                || filter_over.over_clause.is_some()
                || !order_by.is_empty()
                || !within_group.is_empty()
            {
                return Err(invalid(format!("function {name} is not eligible in CHECK")));
            }
            for e in args {
                lower(e, doc, bindings)?;
            }
        }
        _ => {
            return Err(invalid(
                "CHECK disallows database reads, aggregates, and this expression form",
            ))
        }
    }
    Ok(())
}
impl Connection {
    pub(crate) fn check_definition(&self, field: &Field) -> Result<()> {
        let Some(sql) = &field.check else {
            return Ok(());
        };
        let mut expr = parse_check(sql)?;
        let mut bindings = Vec::new();
        lower(&mut expr, None, &mut bindings)?;
        // Preparation validates supported function signatures even on an empty
        // collection. It never executes an expression or reads a table.
        self.engine
            .prepare(format!("SELECT CASE WHEN ({expr}) THEN 1 ELSE 0 END"))
            .map_err(|e| invalid(format!("invalid CHECK: {e}")))?;
        Ok(())
    }
    pub(crate) fn validate_candidate(&self, collection: &Collection, doc: &Document) -> Result<()> {
        crate::validate_document(collection, doc)?;
        for field in &collection.fields {
            let Some(sql) = &field.check else {
                continue;
            };
            if crate::path_value(doc, &field.path)?.is_none_or(|v| matches!(v, Value::Null)) {
                continue;
            }
            let result = (|| -> Result<bool> {
                let mut expr = parse_check(sql)?;
                let mut bindings = Vec::new();
                lower(&mut expr, Some(doc), &mut bindings)?;
                let mut statement = self
                    .engine
                    .prepare(format!("SELECT CASE WHEN ({expr}) THEN 1 ELSE 0 END"))?;
                for (i, value) in bindings.into_iter().enumerate() {
                    statement
                        .bind_at(NonZeroUsize::new(i + 1).expect("one-based binding"), value)?;
                }
                let rows = crate::collect_rows(&mut statement)?;
                Ok(matches!(
                    rows.first().and_then(|row| row.first()),
                    Some(EngineValue::Numeric(turso_core::Numeric::Integer(1)))
                ))
            })();
            match result {
                Ok(true) => {}
                Ok(false) => {
                    return Err(invalid(format!(
                        "CHECK failed for field {}",
                        field.path.join(".")
                    )))
                }
                Err(error) => {
                    return Err(invalid(format!(
                        "CHECK failed for field {}: {error}",
                        field.path.join(".")
                    )))
                }
            }
        }
        Ok(())
    }
}
