use super::execute::{
    execute_demo_attempt, load_active_attempt, request_demo_termination,
    write_latest_attempt_receipt,
};
use super::query::{
    build_demo_groups, build_demo_record, demo_list_query_to_json, demo_list_query_to_key_values,
    demo_matches_query, query_is_empty,
};
use super::*;

#[path = "render/attempts.rs"]
mod attempts;
#[path = "render/registry.rs"]
mod registry;
#[path = "render/terminal.rs"]
mod terminal;

#[cfg(test)]
pub(super) use attempts::render_demo_execute_text;
pub(super) use attempts::{render_demo_execute, render_demo_history, render_demo_stop};
pub(super) use registry::{render_demo_inspect, render_demo_list};
pub(super) use terminal::{render_demo_input, render_demo_resize};
