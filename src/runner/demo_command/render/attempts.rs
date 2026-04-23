#[path = "attempts/execute.rs"]
mod execute;
#[path = "attempts/history.rs"]
mod history;
#[path = "attempts/stop.rs"]
mod stop;

pub(in crate::runner::demo_command) use execute::render_demo_execute;
pub(in crate::runner::demo_command) use history::render_demo_history;
pub(in crate::runner::demo_command) use stop::render_demo_stop;
