use serde::Serialize;

use super::super::row_projection::{builtin_task_rows, BuiltinTaskProjection};

#[derive(Clone, Serialize)]
pub(super) struct BuiltinTaskJsonRow {
    task: String,
    description: String,
}

pub(super) fn builtin_task_rows_json() -> Vec<BuiltinTaskJsonRow> {
    builtin_rows_json(builtin_task_rows())
}

pub(super) fn builtin_rows_json<'a, I>(rows: I) -> Vec<BuiltinTaskJsonRow>
where
    I: IntoIterator<Item = BuiltinTaskProjection<'a>>,
{
    rows.into_iter()
        .map(|(task, description)| BuiltinTaskJsonRow::new(task, description))
        .collect::<Vec<BuiltinTaskJsonRow>>()
}

impl BuiltinTaskJsonRow {
    fn new(task: &str, description: &str) -> Self {
        Self {
            task: task.to_owned(),
            description: description.to_owned(),
        }
    }
}
