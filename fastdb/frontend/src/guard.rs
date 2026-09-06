//! Guard-only AST normalization. Never execute this redacted representation.
use turso_core::{Result, WalkControl};
use turso_parser::ast::*;

pub(crate) fn tokens(sql: &str) -> crate::Result<Vec<fastql_parser::Token>> {
    let Ok(mut cmd) = crate::select::parsed(sql) else {
        return Ok(fastql_parser::tokenize(sql)?);
    };
    let statement = match &mut cmd {
        Cmd::Stmt(s) | Cmd::Explain(s) | Cmd::ExplainQueryPlan(s) => s,
    };
    match statement {
        Stmt::Select(s) => select(s)?,
        Stmt::Insert {
            with,
            body,
            returning,
            ..
        } => {
            ctes(with)?;
            if let InsertBody::Select(s, _) = body {
                select(s)?;
            }
            projections(returning)?;
        }
        Stmt::Update(s) => {
            ctes(&mut s.with)?;
            for set in &mut s.sets {
                expression(&mut set.expr)?;
            }
            from(&mut s.from)?;
            optional(&mut s.where_clause)?;
            projections(&mut s.returning)?;
            ordering(&mut s.order_by)?;
        }
        Stmt::Delete {
            with,
            where_clause,
            returning,
            order_by,
            ..
        } => {
            ctes(with)?;
            optional(where_clause)?;
            projections(returning)?;
            ordering(order_by)?;
        }
        Stmt::CreateView { select: s, .. }
        | Stmt::CreateTable {
            body: CreateTableBody::AsSelect(s),
            ..
        } => select(s)?,
        // PRAGMA, DDL names/constraints, trigger bodies, and other contexts
        // retain the conservative guard until their reference roles are covered.
        _ => {}
    }
    Ok(fastql_parser::tokenize(&cmd.to_string())?)
}
fn expression(expr: &mut Expr) -> Result<()> {
    turso_core::walk_expr_mut(expr, &mut |expr| {
        match expr {
            Expr::Literal(Literal::String(_)) => {
                *expr = Expr::Literal(Literal::Numeric("0".into()));
            }
            Expr::Exists(s) | Expr::Subquery(s) => select(s)?,
            Expr::InSelect { rhs, .. } => select(rhs)?,
            _ => {}
        }
        Ok(WalkControl::Continue)
    })?;
    Ok(())
}
fn optional(expr: &mut Option<Box<Expr>>) -> Result<()> {
    if let Some(expr) = expr {
        expression(expr)?;
    }
    Ok(())
}
fn projections(columns: &mut [ResultColumn]) -> Result<()> {
    for column in columns {
        if let ResultColumn::Expr(expr, _) = column {
            expression(expr)?;
        }
    }
    Ok(())
}
fn ordering(columns: &mut [SortedColumn]) -> Result<()> {
    for column in columns {
        expression(&mut column.expr)?;
    }
    Ok(())
}
fn ctes(with: &mut Option<With>) -> Result<()> {
    if let Some(with) = with {
        for cte in &mut with.ctes {
            select(&mut cte.select)?;
        }
    }
    Ok(())
}
fn table(source: &mut SelectTable) -> Result<()> {
    match source {
        SelectTable::Select(s, _) => select(s)?,
        SelectTable::Sub(s, _) => from_clause(s)?,
        // Table-function arguments can designate objects (e.g. PRAGMA tables).
        SelectTable::Table(..) | SelectTable::TableCall(..) => {}
    }
    Ok(())
}
fn from_clause(source: &mut FromClause) -> Result<()> {
    table(&mut source.select)?;
    for join in &mut source.joins {
        table(&mut join.table)?;
        if let Some(JoinConstraint::On(expr)) = &mut join.constraint {
            expression(expr)?;
        }
    }
    Ok(())
}
fn from(source: &mut Option<FromClause>) -> Result<()> {
    if let Some(source) = source {
        from_clause(source)?;
    }
    Ok(())
}
fn core(core: &mut OneSelect) -> Result<()> {
    match core {
        OneSelect::Values(rows) => {
            for row in rows {
                for expr in row {
                    expression(expr)?;
                }
            }
        }
        OneSelect::Select {
            columns,
            from: source,
            where_clause,
            group_by,
            ..
        } => {
            projections(columns)?;
            from(source)?;
            optional(where_clause)?;
            if let Some(group) = group_by {
                for expr in &mut group.exprs {
                    expression(expr)?;
                }
                optional(&mut group.having)?;
            }
        }
    }
    Ok(())
}
fn select(select: &mut Select) -> Result<()> {
    ctes(&mut select.with)?;
    core(&mut select.body.select)?;
    for compound in &mut select.body.compounds {
        core(&mut compound.select)?;
    }
    ordering(&mut select.order_by)?;
    Ok(())
}
