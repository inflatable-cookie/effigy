#[path = "tasks_view/probe_view.rs"]
mod probe_view;
#[path = "tasks_view/profile_rows.rs"]
mod profile_rows;

pub(super) type ManagedProfileDisplayRow = profile_rows::ManagedProfileDisplayRow;

pub(super) fn relative_display_path(root: &std::path::Path, path: &std::path::Path) -> String {
    profile_rows::relative_display_path(root, path)
}

pub(super) fn managed_profile_display_rows(
    catalog: &super::LoadedCatalog,
    task_name: &str,
    task: &super::ManifestTask,
) -> Vec<ManagedProfileDisplayRow> {
    profile_rows::managed_profile_display_rows(catalog, task_name, task)
}

pub(super) fn style_text(enabled: bool, style: anstyle::Style, text: &str) -> String {
    if !enabled {
        return text.to_owned();
    }
    format!("{}{}{}", style.render(), text, style.render_reset())
}

pub(super) fn render_resolution_probe_block(
    renderer: &mut crate::ui::PlainRenderer<Vec<u8>>,
    probe: &serde_json::Value,
    color_enabled: bool,
    show_evidence: bool,
) -> Result<(), super::RunnerError> {
    probe_view::render_resolution_probe_block(renderer, probe, color_enabled, show_evidence)
}
