//! Guard-only AST normalization. Never execute this redacted representation.
use turso_core::{Result, WalkControl};
use turso_parser::ast::*;

// FastQL is not always parseable before expansion. Inspect the original token
// roles so generated helper names never need an exception here. SQLite also
// accepts single-quoted function names; ordinary string values remain data.
pub(crate) fn internal_names(sql: &str) -> crate::Result<()> {
    use fastql_parser::Kind;
    let tokens = fastql_parser::tokenize(sql)?;
    for (i, token) in tokens.iter().enumerate() {
        let name = matches!(token.kind, Kind::Word | Kind::Identifier)
            || (token.kind == Kind::String
                && tokens
                    .get(i + 1)
                    .is_some_and(|next| matches!(next.text.as_str(), "(" | ".")));
        if name && token.text.to_ascii_lowercase().starts_with("__fastdb_") {
            return Err(crate::Error::Unsupported("managed names".into()));
        }
    }
    Ok(())
}

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
            if let InsertBody::Select(s, conflict) = body {
                select(s)?;
                upserts(conflict)?;
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
        Stmt::CreateTable {
            body:
                CreateTableBody::ColumnsAndConstraints {
                    columns,
                    constraints,
                    ..
                },
            ..
        } => {
            for column in columns {
                column_definition(column)?;
            }
            for constraint in constraints {
                match &mut constraint.constraint {
                    TableConstraint::Check(expr) => expression(expr)?,
                    TableConstraint::PrimaryKey { columns, .. }
                    | TableConstraint::Unique { columns, .. } => ordering(columns)?,
                    TableConstraint::ForeignKey { .. } => {}
                }
            }
        }
        Stmt::CreateIndex {
            columns,
            where_clause,
            ..
        } => {
            ordering(columns)?;
            optional(where_clause)?;
        }
        Stmt::AlterTable(AlterTable {
            body:
                AlterTableBody::AddColumn(column) | AlterTableBody::AlterColumn { new: column, .. },
            ..
        }) => column_definition(column)?,
        Stmt::CreateTrigger {
            when_clause,
            commands,
            ..
        } => {
            optional(when_clause)?;
            for command in commands {
                trigger_command(command)?;
            }
        }
        // PRAGMA, DDL names, and other uncovered contexts
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

fn column_definition(column: &mut ColumnDefinition) -> Result<()> {
    for constraint in &mut column.constraints {
        match &mut constraint.constraint {
            ColumnConstraint::Default(expr)
            | ColumnConstraint::Check(expr)
            | ColumnConstraint::Generated { expr, .. } => expression(expr)?,
            _ => {}
        }
    }
    Ok(())
}
fn upserts(conflict: &mut Option<Box<Upsert>>) -> Result<()> {
    let mut current = conflict.as_deref_mut();
    while let Some(clause) = current {
        if let Some(index) = &mut clause.index {
            ordering(&mut index.targets)?;
            optional(&mut index.where_clause)?;
        }
        if let UpsertDo::Set { sets, where_clause } = &mut clause.do_clause {
            for set in sets {
                expression(&mut set.expr)?;
            }
            optional(where_clause)?;
        }
        current = clause.next.as_deref_mut();
    }
    Ok(())
}

fn trigger_command(command: &mut TriggerCmd) -> Result<()> {
    match command {
        TriggerCmd::Select(s) => select(s)?,
        TriggerCmd::Insert {
            select: s,
            upsert,
            returning,
            ..
        } => {
            select(s)?;
            upserts(upsert)?;
            projections(returning)?;
        }
        TriggerCmd::Update {
            sets,
            from: source,
            where_clause,
            ..
        } => {
            for set in sets {
                expression(&mut set.expr)?;
            }
            from(source)?;
            optional(where_clause)?;
        }
        TriggerCmd::Delete { where_clause, .. } => optional(where_clause)?,
    }
    Ok(())
}
