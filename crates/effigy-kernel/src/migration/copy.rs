//! One destination transaction copies only the selected relational closure.
use super::plan::has_column;
use super::*;
use crate::ctx::EffectContext;
use crate::store::StoreReader;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter};
use std::path::Path;

const TABLES: [&str; 10] = [
    "checkouts",
    "resources",
    "runs",
    "events",
    "attachments",
    "claims",
    "requests",
    "objects",
    "object_refs",
    "logs",
];

#[derive(Debug, Default, Serialize)]
pub(super) struct Copied {
    pub rows: BTreeMap<String, u64>,
}

pub(super) fn partition(
    cx: &EffectContext,
    source: &StoreReader,
    destination: &mut Store,
    part: &Partition,
    final_state: &Path,
    migration: &str,
    source_digest: &str,
) -> Result<Copied, StoreError> {
    // Never overlay a destination which already owns product state. Prepared
    // retries use their own migration-tagged staging root, not this primitive.
    for table in TABLES {
        let count: i64 =
            destination
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        if count != 0 {
            return Err(StoreError::Corrupt(format!(
                "migration destination {table} is not empty"
            )));
        }
    }
    destination.txn(cx, |txn| {
        txn.conn().execute_batch("PRAGMA defer_foreign_keys=ON")?;
        let mut copied = Copied::default();
        for table in TABLES {
            cx.operation()
                .check("legacy destination transaction")
                .map_err(StoreError::Interrupted)?;
            let mut query = source.conn().prepare(&format!("SELECT * FROM {table}"))?;
            let columns: Vec<String> = query
                .column_names()
                .into_iter()
                .map(str::to_owned)
                .collect();
            for column in &columns {
                if !has_column(txn.conn(), table, column)? {
                    return Err(StoreError::Corrupt(format!(
                        "unsupported legacy {table}.{column}; destination cannot preserve it"
                    )));
                }
            }
            let column_list = columns
                .iter()
                .map(|name| format!("\"{}\"", name.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(",");
            let values = (0..columns.len())
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let mut insert = txn.conn().prepare_cached(&format!(
                "INSERT INTO {table}({column_list}) VALUES ({values})"
            ))?;
            let mut rows = query.query([])?;
            while let Some(row) = rows.next()? {
                cx.operation()
                    .check("legacy row copy")
                    .map_err(StoreError::Interrupted)?;
                let mut values = (0..columns.len())
                    .map(|i| row.get::<_, Value>(i))
                    .collect::<Result<Vec<_>, _>>()?;
                if !selected(table, &columns, &values, part)? {
                    continue;
                }
                if table == "objects" {
                    let digest = text(&columns, &values, "digest")?;
                    let path = final_state.join("objects").join(digest);
                    let path = path.to_str().ok_or_else(|| {
                        StoreError::Corrupt("object destination path is not UTF-8".into())
                    })?;
                    let index = column(&columns, "path")?;
                    values[index] = Value::Text(path.into());
                }
                insert.execute(params_from_iter(values))?;
                *copied.rows.entry(table.into()).or_default() += 1;
            }
        }
        for (key, value) in [
            ("migration_id", migration),
            ("migration_source_digest", source_digest),
            ("migration_format", "q010-v1"),
        ] {
            txn.conn().execute(
                "INSERT INTO meta(key,value) VALUES (?1,?2)",
                params![key, value],
            )?;
        }
        Ok(copied)
    })
}

fn column(columns: &[String], name: &str) -> Result<usize, StoreError> {
    columns
        .iter()
        .position(|column| column == name)
        .ok_or_else(|| StoreError::Corrupt(format!("legacy column {name} is missing")))
}
fn text<'a>(columns: &[String], values: &'a [Value], name: &str) -> Result<&'a str, StoreError> {
    match &values[column(columns, name)?] {
        Value::Text(value) => Ok(value),
        _ => Err(StoreError::Corrupt(format!(
            "legacy column {name} is not text"
        ))),
    }
}
fn selected(
    table: &str,
    columns: &[String],
    values: &[Value],
    part: &Partition,
) -> Result<bool, StoreError> {
    Ok(match table {
        "checkouts" => match part.checkout {
            Some(checkout) => text(columns, values, "token")? == crate::store::token(&checkout),
            None => false,
        },
        "resources" => part.resources.contains(text(columns, values, "name")?),
        "runs" => part.runs.contains(text(columns, values, "run_id")?),
        "events" | "attachments" | "requests" => {
            part.runs.contains(text(columns, values, "run_id")?)
        }
        "logs" => part.checkout.is_some() && part.runs.contains(text(columns, values, "run_id")?),
        "claims" => {
            part.resources.contains(text(columns, values, "resource")?)
                && text(columns, values, "state")? != "released"
        }
        "objects" => part.objects.contains(text(columns, values, "digest")?),
        "object_refs" => {
            part.resources.contains(text(columns, values, "resource")?)
                && part.objects.contains(text(columns, values, "digest")?)
        }
        _ => false,
    })
}
