use effigy_execution::ExecutionSelectionPlan;
use effigy_manifest::TaskSelection;

pub(in crate::runner) enum SelectionResolution<'a> {
    Selected {
        selection: TaskSelection<'a>,
        plan: Box<ExecutionSelectionPlan>,
    },
    Output(String),
}

pub(in crate::runner) fn selected(
    selection: TaskSelection<'_>,
    plan: ExecutionSelectionPlan,
) -> SelectionResolution<'_> {
    SelectionResolution::Selected {
        selection,
        plan: Box::new(plan),
    }
}

pub(in crate::runner) fn output(rendered: String) -> SelectionResolution<'static> {
    SelectionResolution::Output(rendered)
}
