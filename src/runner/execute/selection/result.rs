use crate::runner::model::catalog::TaskSelection;

pub(in crate::runner) enum SelectionResolution<'a> {
    Selected(TaskSelection<'a>),
    Output(String),
}

pub(in crate::runner) fn selected(selection: TaskSelection<'_>) -> SelectionResolution<'_> {
    SelectionResolution::Selected(selection)
}

pub(in crate::runner) fn output(rendered: String) -> SelectionResolution<'static> {
    SelectionResolution::Output(rendered)
}
