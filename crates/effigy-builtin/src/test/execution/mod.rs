//! Built-in test suite execution: which runner a run uses, and the two ways
//! suites run.
//!
//! The interactive session lives in [`session`], the parallel runner in
//! [`parallel`], and every suite command's rendering (Cargo env wrap, lifecycle
//! script, container routing, sweep custody) in [`command`].

use std::io::IsTerminal;

mod command;
mod parallel;
mod session;

pub(super) use parallel::run_builtin_test_targets_parallel;
pub(super) use session::{run_builtin_test_targets_tui, BuiltinTestSession};

#[cfg(test)]
use parallel::suite_outcome;
#[cfg(test)]
use session::{builtin_test_results_from_tui, suite_result};

pub(super) fn should_run_builtin_test_tui(
    force_tui: bool,
    suite_count: usize,
    output_json: bool,
) -> bool {
    // JSON output is a machine contract: the TUI would interleave terminal
    // control output with the payload on stdout.
    if output_json || !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return false;
    }
    force_tui || suite_count > 1
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
