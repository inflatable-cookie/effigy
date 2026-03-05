use super::row_projection::BuiltinTaskRow;

#[derive(Clone, Copy)]
pub(super) struct PreparedBuiltinTaskRow<'a> {
    task: &'a str,
    description: &'a str,
}

impl<'a> PreparedBuiltinTaskRow<'a> {
    fn new(task: &'a str, description: &'a str) -> Self {
        Self { task, description }
    }

    pub(super) fn task(&self) -> &'a str {
        self.task
    }

    pub(super) fn description(&self) -> &'a str {
        self.description
    }
}

pub(super) fn prepare_builtin_task_rows<'a>(
    rows: impl IntoIterator<Item = BuiltinTaskRow<'a>>,
) -> Vec<PreparedBuiltinTaskRow<'a>> {
    rows.into_iter()
        .map(|row| PreparedBuiltinTaskRow::new(row.task(), row.description()))
        .collect()
}
