//! Forward-only migrations with transactional exact-source history.
use crate::{Connection, Error, Parameters, Result, TransactionState};
use serde::Serialize;
#[derive(Clone, Debug)]
pub struct Migration {
    pub version: i64,
    pub name: String,
    pub sql: String,
}
#[derive(Debug, Serialize)]
pub struct MigrationReport {
    pub already_applied: usize,
    pub applied: Vec<i64>,
}
impl Connection {
    /// Supply the complete, strictly version-ordered history and pending scripts.
    /// All pending scripts and history rows commit in one transaction.
    pub fn migrate(&self, migrations: &[Migration]) -> Result<MigrationReport> {
        if self.transaction_state() != TransactionState::Autocommit {
            return Err(Error::Validation(
                "migration runner requires autocommit".into(),
            ));
        }
        if migrations.len() > 1000
            || migrations.iter().map(|m| m.sql.len()).sum::<usize>() > 16 * 1024 * 1024
        {
            return Err(Error::Limit(
                "migration plan exceeds 1000 entries or 16 MiB".into(),
            ));
        }
        let mut previous = 0;
        let mut scripts = Vec::new();
        for migration in migrations {
            if migration.version <= previous
                || migration.name.is_empty()
                || migration.name.len() > 255
                || migration.name.contains('\0')
            {
                return Err(Error::Validation("migration versions must be positive and strictly increasing; names require 1–255 bytes".into()));
            }
            previous = migration.version;
            if migration.sql.len() > 4 * 1024 * 1024 {
                return Err(Error::Limit("migration script exceeds 4 MiB".into()));
            }
            let statements = fastql_parser::split_script(&migration.sql)?;
            for statement in &statements {
                let tokens = fastql_parser::tokenize(statement.sql)?;
                let first = tokens.first().expect("nonempty statement");
                if first.kind != fastql_parser::Kind::Word
                    || !matches!(
                        first.text.to_ascii_uppercase().as_str(),
                        "CREATE"
                            | "ALTER"
                            | "DROP"
                            | "INSERT"
                            | "UPDATE"
                            | "DELETE"
                            | "UPSERT"
                            | "DEFINE"
                            | "REMOVE"
                            | "SELECT"
                            | "WITH"
                            | "REINDEX"
                            | "ANALYZE"
                    )
                {
                    return Err(Error::Validation(format!("migration {} at byte {}: statement is not eligible for managed migration transactions",migration.version,statement.offset)));
                }
                if first.text.eq_ignore_ascii_case("CREATE")
                    && tokens.get(1).is_some_and(|t| {
                        t.text.eq_ignore_ascii_case("TEMP")
                            || t.text.eq_ignore_ascii_case("TEMPORARY")
                    })
                {
                    return Err(Error::Validation(
                        "temporary objects are not migration history".into(),
                    ));
                }
            }
            scripts.push(statements);
        }
        self.atomic(|| {
            self.run("CREATE TABLE IF NOT EXISTS __fastdb_migrations (version INTEGER PRIMARY KEY, name TEXT NOT NULL, script TEXT NOT NULL)",&[])?;
            let history=self.run("SELECT version,name,script FROM __fastdb_migrations ORDER BY version",&[])?;
            if history.len()>migrations.len() {return Err(Error::Validation("migration plan omits applied history".into()));}
            for (row,migration) in history.iter().zip(migrations) {
                let difference = if row.len() != 3 {
                    Some("invalid history row")
                } else if crate::from_engine(row[0].clone()) != crate::Value::Integer(migration.version) {
                    Some("version does not match the applied sequence")
                } else if crate::from_engine(row[1].clone()) != crate::Value::String(migration.name.clone()) {
                    Some("name differs")
                } else if crate::from_engine(row[2].clone()) != crate::Value::String(migration.sql.clone()) {
                    Some("SQL source differs (including whitespace and comments)")
                } else {
                    None
                };
                if let Some(difference) = difference {
                    return Err(Error::Validation(format!("applied migration history differs at supplied version {}: {difference}",migration.version)));
                }
            }
            let mut applied=Vec::new();
            for (migration,statements) in migrations.iter().zip(&scripts).skip(history.len()) {
                for statement in statements {
                    self.execute(statement.sql,&Parameters::new()).map_err(|source|Error::Migration {
                        version:migration.version,offset:statement.offset,source:Box::new(source)
                    })?;
                }
                self.run("INSERT INTO __fastdb_migrations (version,name,script) VALUES (?1,?2,?3)",
                    &[crate::EngineValue::from_i64(migration.version),crate::text(&migration.name),crate::text(&migration.sql)])?;
                applied.push(migration.version);
            }
            Ok(MigrationReport {already_applied:history.len(),applied})
        })
    }
}
