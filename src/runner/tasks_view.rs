#[path = "tasks_view/probe_view.rs"]
mod probe_view;
#[path = "tasks_view/profile_rows.rs"]
mod profile_rows;
#[path = "tasks_view/style.rs"]
mod style;

pub(super) use probe_view::render_resolution_probe_block;
pub(super) use profile_rows::{
    managed_profile_display_rows, relative_display_path, ManagedProfileDisplayRow,
};
pub(super) use style::style_text;
