use effigy_manifest::TaskSelection;

pub(in crate::runner) enum SelectionResolution<'a> {
    Selected { selection: TaskSelection<'a> },
    Output(String),
}

pub(in crate::runner) fn output(rendered: String) -> SelectionResolution<'static> {
    SelectionResolution::Output(rendered)
}
