use std::path::PathBuf;

#[cfg(test)]
use effigy_demo::browser::DemoSummary;
#[cfg(test)]
use effigy_demo::browser::{
    DemoActionAvailability, DemoActionState, DemoActiveAttempt, DemoActiveTerminalSession,
    DemoHistoryAttemptHistoryPayload, DemoLatestAttempt, DemoRuntimeBackend,
    DemoRuntimeProjectedOutputProvenance, DemoRuntimeProjectedProcessSummary,
    DemoRuntimeProjectionShape, DemoTerminalRecentOutput, DemoTerminalResize, DemoTerminalSize,
};
use effigy_tui::demo_browser::{init_browser_terminal, restore_browser_terminal, DemoBrowserApp};
use serde_json::Value as JsonValue;

use crate::runner::{run_command, RunnerError};
use crate::{Command, DemoArgs, DemoListGroupBy};

pub fn run_demo_browser_tui(
    repo_root: PathBuf,
    initial_group_by: Option<DemoListGroupBy>,
) -> Result<(), RunnerError> {
    let mut terminal = init_browser_terminal().map_err(RunnerError::Ui)?;
    let mut app = DemoBrowserApp::new(repo_root, initial_group_by);
    let result = app
        .run_with(&mut terminal, |args| {
            invoke_demo_json(args).map_err(|error| error.to_string())
        })
        .map_err(RunnerError::Ui);
    restore_browser_terminal(&mut terminal).map_err(RunnerError::Ui)?;
    result
}

fn invoke_demo_json(args: DemoArgs) -> Result<JsonValue, RunnerError> {
    let result = run_command(Command::Demo(args));
    let rendered = match result {
        Ok(rendered) => rendered,
        Err(RunnerError::CommandJsonFailure { rendered }) => rendered,
        Err(error) => return Err(error),
    };
    serde_json::from_str(&rendered).map_err(|error| {
        RunnerError::TaskInvocation(format!("failed to parse demo json payload: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use effigy_demo::browser::{DemoDetail, DemoHistoryAttempt, DemoHistoryPayload};
    use effigy_tui::demo_browser::{
        action_menu_items_for_detail, artifacts_detail_render, browser_body_constraints,
        browser_header_lines, browser_list_summary_lines, browser_live_terminal_env,
        browser_vt_lines, first_demo_id, history_detail_render, overview_detail_render,
        query_summary, read_recent_log_lines, render_browser_demo_row, resolve_repo_relative_path,
        row_contains_demo, sanitize_live_terminal_bytes, selected_list_highlight_style,
        selected_list_highlight_symbol, take_complete_terminal_bytes, terminal_detail_render,
        BrowserFocus, DetailSelectableItem, DEMO_BROWSER_TERMINAL_COLS_ENV,
        DEMO_BROWSER_TERMINAL_PARSER_SCROLLBACK, DEMO_BROWSER_TERMINAL_ROWS_ENV,
    };
    use ratatui::{
        layout::Constraint,
        style::{Color, Modifier},
        text::Line,
    };
    use serde_json::Value as JsonValue;

    use super::{
        DemoArgs, DemoBrowserApp, DemoHistoryAttemptHistoryPayload, DemoLatestAttempt,
        DemoListGroupBy, DemoSummary,
    };
    use effigy_cli::{DemoListGap, DemoListMode, DemoListQuery, DemoListStatus};
    use effigy_tui::demo_browser::{
        browser_terminal_key_input, clamp_artifact_index, detail_prefers_live_browser_terminal,
        detail_tab_lines, next_gap_filter, next_group_by, next_mode_filter, next_status_filter,
        resolve_artifact_path, selected_artifact, ActionMenuItem, BrowserRow, DetailTab,
    };

    fn unexpected_invoke(_: DemoArgs) -> Result<JsonValue, String> {
        Err("unexpected demo invocation".to_owned())
    }

    fn success_invoke(_: DemoArgs) -> Result<JsonValue, String> {
        Ok(serde_json::json!({}))
    }

    fn summary(id: &str) -> DemoSummary {
        DemoSummary {
            id: id.to_owned(),
            effective_status: "ready".to_owned(),
            ..Default::default()
        }
    }

    fn detail_with_artifacts(artifacts: &[&str]) -> DemoDetail {
        DemoDetail {
            id: "demo".to_owned(),
            title: "Demo".to_owned(),
            summary: "summary".to_owned(),
            proof: "proof".to_owned(),
            owner: "owner".to_owned(),
            mode: "headless".to_owned(),
            effective_status: "ready".to_owned(),
            gap_class: "existing".to_owned(),
            covers: vec![],
            tags: vec![],
            runtime_backend: super::DemoRuntimeBackend {
                kind: "task".to_owned(),
                label: "task-backed".to_owned(),
                flattened_projection: false,
                projection_shape: super::DemoRuntimeProjectionShape::default(),
                projected_process_summary: super::DemoRuntimeProjectedProcessSummary::default(),
                projected_output_provenance: super::DemoRuntimeProjectedOutputProvenance::default(),
                capabilities: vec![],
            },
            actions: super::DemoActionAvailability {
                run: super::DemoActionState {
                    available: true,
                    reason: None,
                },
                stop: super::DemoActionState {
                    available: false,
                    reason: None,
                },
                rerun: super::DemoActionState {
                    available: true,
                    reason: None,
                },
            },
            active_attempt: super::DemoActiveAttempt {
                state: "idle".to_owned(),
                ..Default::default()
            },
            active_terminal_session: super::DemoActiveTerminalSession {
                available: false,
                state: "idle".to_owned(),
                transport: "none".to_owned(),
                pty: false,
                supports_input_forwarding: false,
                input_forwarding_reason: Some(
                    "Input forwarding is not available for this active demo.".to_owned(),
                ),
                terminal_size: super::DemoTerminalSize {
                    cols: None,
                    rows: None,
                },
                resize: super::DemoTerminalResize {
                    available: false,
                    ..Default::default()
                },
                output_available: false,
                recent_output: super::DemoTerminalRecentOutput {
                    stdout_lines: vec![],
                    stderr_lines: vec![],
                },
                ..Default::default()
            },
            latest_attempt: DemoLatestAttempt {
                recorded: true,
                state: "passed".to_owned(),
                artifacts: artifacts.iter().map(|value| (*value).to_owned()).collect(),
                summary: None,
                output_available: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn browser_group_cycle_is_bounded() {
        assert_eq!(next_group_by(None), Some(DemoListGroupBy::Owner));
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Owner)),
            Some(DemoListGroupBy::Tag)
        );
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Tag)),
            Some(DemoListGroupBy::Mode)
        );
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Mode)),
            Some(DemoListGroupBy::Cover)
        );
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Cover)),
            Some(DemoListGroupBy::Status)
        );
        assert_eq!(
            next_group_by(Some(DemoListGroupBy::Status)),
            Some(DemoListGroupBy::Gap)
        );
        assert_eq!(next_group_by(Some(DemoListGroupBy::Gap)), None);
    }

    #[test]
    fn browser_mode_filter_cycle_is_bounded() {
        assert_eq!(next_mode_filter(None), Some(DemoListMode::Headless));
        assert_eq!(
            next_mode_filter(Some(DemoListMode::Headless)),
            Some(DemoListMode::Interactive)
        );
        assert_eq!(
            next_mode_filter(Some(DemoListMode::Interactive)),
            Some(DemoListMode::Hybrid)
        );
        assert_eq!(next_mode_filter(Some(DemoListMode::Hybrid)), None);
    }

    #[test]
    fn browser_row_helpers_ignore_group_headers() {
        let rows = vec![
            BrowserRow::Group("ready".to_owned()),
            BrowserRow::Demo(summary("alpha")),
            BrowserRow::Demo(summary("beta")),
        ];
        assert_eq!(first_demo_id(&rows).as_deref(), Some("alpha"));
        assert!(row_contains_demo(&rows, "beta"));
        assert!(!row_contains_demo(&rows, "missing"));
    }

    #[test]
    fn browser_demo_rows_do_not_show_redundant_bracketed_action_summary() {
        let summary = summary("browser-proof-report");
        let line = render_browser_demo_row(&summary, 32);
        let rendered = line.to_string();

        assert!(rendered.contains("browser-proof-report"));
        assert!(rendered.contains("ready"));
        assert!(!rendered.contains('['));
        assert!(!rendered.contains("run/rerun"));
    }

    #[test]
    fn browser_demo_rows_ellipsize_name_and_preserve_right_aligned_status() {
        let summary = summary("hardware-topology-diagnostics");
        let rendered = render_browser_demo_row(&summary, 24).to_string();

        assert!(rendered.contains("hardware-topol..."));
        assert!(rendered.ends_with("ready "));
    }

    #[test]
    fn browser_selected_artifact_clamps_to_available_range() {
        let detail = detail_with_artifacts(&["one", "two"]);
        assert_eq!(clamp_artifact_index(0, &detail), 0);
        assert_eq!(clamp_artifact_index(5, &detail), 1);
        assert_eq!(selected_artifact(&detail, 5), Some("two"));
    }

    #[test]
    fn browser_resolves_relative_artifacts_against_repo_root() {
        let repo_root = Path::new("/tmp/demo-repo");
        assert_eq!(
            resolve_artifact_path(repo_root, ".effigy/demo/report.html"),
            repo_root.join(".effigy/demo/report.html")
        );
    }

    #[test]
    fn browser_resolves_generic_repo_relative_paths() {
        let repo_root = Path::new("/tmp/demo-repo");
        assert_eq!(
            resolve_repo_relative_path(repo_root, ".effigy/demo/logs/demo.stdout.log"),
            repo_root.join(".effigy/demo/logs/demo.stdout.log")
        );
    }

    #[test]
    fn browser_reads_only_recent_log_lines() {
        let temp_path = std::env::temp_dir().join(format!(
            "effigy-demo-browser-log-{}.txt",
            std::process::id()
        ));
        std::fs::write(
            &temp_path,
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\n",
        )
        .expect("write log");
        let lines = read_recent_log_lines(&temp_path, 4).expect("read log");
        let _ = std::fs::remove_file(&temp_path);
        assert_eq!(lines, vec!["six", "seven", "eight", "nine"]);
    }

    #[test]
    fn browser_status_filter_cycle_is_bounded() {
        assert_eq!(next_status_filter(None), Some(DemoListStatus::Planned));
        assert_eq!(
            next_status_filter(Some(DemoListStatus::Broken)),
            Some(DemoListStatus::Missing)
        );
        assert_eq!(next_status_filter(Some(DemoListStatus::Missing)), None);
    }

    #[test]
    fn browser_gap_filter_cycle_is_bounded() {
        assert_eq!(next_gap_filter(None), Some(DemoListGap::Existing));
        assert_eq!(
            next_gap_filter(Some(DemoListGap::Broken)),
            Some(DemoListGap::Stale)
        );
        assert_eq!(next_gap_filter(Some(DemoListGap::Stale)), None);
    }

    #[test]
    fn browser_query_summary_is_human_readable() {
        let query = DemoListQuery {
            search: Some("auth".to_owned()),
            owner: Some("signal".to_owned()),
            tag: Some("self-hosted".to_owned()),
            mode: Some(DemoListMode::Interactive),
            cover: Some("effigy.demo.lifecycle".to_owned()),
            status: Some(DemoListStatus::Ready),
            gap: Some(DemoListGap::Existing),
            stale_only: true,
            ..DemoListQuery::default()
        };
        assert_eq!(
            query_summary(&query),
            "search=auth, owner=signal, tag=self-hosted, mode=interactive, cover=effigy.demo.lifecycle, status=ready, gap=existing, stale-only=true"
        );
        assert_eq!(query_summary(&DemoListQuery::default()), "none");
    }

    #[test]
    fn browser_detail_lines_use_compact_sections() {
        let mut detail =
            detail_with_artifacts(&[".effigy/demo/artifacts/browser-proof-report/index.html"]);
        detail.id = "browser-proof-report".to_owned();
        detail.title = "Browser Proof Report".to_owned();
        detail.summary = "Generate a human-checkable proof report.".to_owned();
        detail.proof = "Verify the browser-facing proof path stays inspectable.".to_owned();
        detail.owner = "effigy".to_owned();
        detail.covers = vec!["effigy.demo.browser".to_owned()];
        detail.tags = vec!["self-hosted".to_owned(), "proof".to_owned()];
        detail.latest_attempt.summary = Some("Latest attempt wrote a proof report.".to_owned());

        let rendered = overview_detail_render(
            &detail,
            Some(DetailSelectableItem::Action(ActionMenuItem::Rerun)),
            true,
            false,
        )
        .lines
        .into_iter()
        .map(|line: Line<'static>| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

        assert!(rendered.contains("Summary"));
        assert!(rendered.contains("Generate a human-checkable proof report."));
        assert!(rendered.contains("tags: self-hosted, proof"));
        assert!(rendered.contains("Actions"));
        assert!(rendered.contains("Rerun demo"));
        assert!(rendered.contains("covers: effigy.demo.browser"));
        assert!(!rendered.contains("Browser Proof Report"));
        assert!(
            rendered.find("tags: self-hosted, proof")
                < rendered.find("covers: effigy.demo.browser")
        );
        assert!(rendered.find("covers: effigy.demo.browser") < rendered.find("Summary"));
        assert!(!rendered.contains("Result"));
        assert!(!rendered.contains("status: passed"));
        assert!(!rendered.contains("Latest attempt wrote a proof report."));
        assert!(!rendered.contains("Latest Receipt"));
        assert!(!rendered.contains("actions:"));
        assert!(!rendered.contains("attempts:"));
        assert!(!rendered.contains("Artifacts"));
    }

    #[test]
    fn browser_detail_lines_hide_pointer_when_inactive() {
        let detail = detail_with_artifacts(&[]);

        let rendered = overview_detail_render(
            &detail,
            Some(DetailSelectableItem::Action(ActionMenuItem::Rerun)),
            false,
            false,
        )
        .lines
        .into_iter()
        .map(|line: Line<'static>| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

        assert!(!rendered.contains("› Rerun demo"));
    }

    #[test]
    fn browser_tab_line_renders_all_demo_scoped_tabs() {
        let rendered = detail_tab_lines(DetailTab::Terminal, true, 32)
            .into_iter()
            .map(|line: Line<'static>| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Overview"));
        assert!(rendered.contains("History"));
        assert!(rendered.contains("Terminal"));
        assert!(rendered.contains("Artifacts"));
        assert!(!rendered.contains("tabs:"));
        assert!(rendered.contains(" Terminal "));
        assert!(rendered.contains("─"));
    }

    #[test]
    fn browser_tab_border_matches_requested_width() {
        let lines = detail_tab_lines(DetailTab::Overview, true, 17);
        assert_eq!(lines[1].to_string().chars().count(), 17);
    }

    #[test]
    fn browser_header_lines_only_show_repo_context() {
        let rendered = browser_header_lines(Path::new("/tmp/demo-repo"))
            .into_iter()
            .map(|line: Line<'static>| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Demo Browser"));
        assert!(rendered.contains("repo: /tmp/demo-repo"));
        assert!(!rendered.contains("group:"));
        assert!(!rendered.contains("pending:"));
        assert!(!rendered.contains("query:"));
        assert!(!rendered.contains("count:"));
    }

    #[test]
    fn browser_list_summary_lines_show_query_and_count() {
        let rendered = browser_list_summary_lines("ready only", 2, 7)
            .into_iter()
            .map(|line: Line<'static>| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("query: ready only"));
        assert!(rendered.contains("count: 2/7"));
    }

    #[test]
    fn browser_body_constraints_keep_stable_wide_split() {
        let constraints = browser_body_constraints();
        assert_eq!(
            constraints,
            [Constraint::Percentage(28), Constraint::Percentage(72)]
        );
    }

    #[test]
    fn browser_list_selection_style_persists_when_detail_is_focused() {
        let unfocused = selected_list_highlight_style(false);
        let focused = selected_list_highlight_style(true);

        assert_eq!(selected_list_highlight_symbol(), "▌");
        assert_eq!(unfocused.fg, Some(effigy_tui::core::EFFIGY_ACCENT_SOFT));
        assert_eq!(focused.fg, Some(Color::Yellow));
        assert!(unfocused.add_modifier.contains(Modifier::BOLD));
        assert!(focused.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn browser_detail_lines_show_result_only_after_session_run_visibility() {
        let mut detail = detail_with_artifacts(&["one"]);
        detail.latest_attempt.summary = Some("Latest attempt wrote a proof report.".to_owned());

        let hidden = overview_detail_render(
            &detail,
            Some(DetailSelectableItem::Action(ActionMenuItem::Rerun)),
            true,
            false,
        )
        .lines
        .into_iter()
        .map(|line: Line<'static>| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        assert!(!hidden.contains("Result"));

        let visible = overview_detail_render(
            &detail,
            Some(DetailSelectableItem::Action(ActionMenuItem::Rerun)),
            true,
            true,
        )
        .lines
        .into_iter()
        .map(|line: Line<'static>| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        assert!(visible.contains("Result"));
        assert!(visible.contains("status: passed"));
        assert!(visible.contains("Latest attempt wrote a proof report."));
        assert!(visible.find("Actions") < visible.find("Result"));
    }

    #[test]
    fn browser_action_menu_keeps_tab_switching_out_of_action_menu() {
        let detail = detail_with_artifacts(&["one", "two"]);
        let items = action_menu_items_for_detail(&detail)
            .into_iter()
            .map(ActionMenuItem::label)
            .collect::<Vec<_>>();

        assert_eq!(items, vec!["Rerun demo", "Refresh state"]);
    }

    #[test]
    fn browser_terminal_view_renders_active_session_output() {
        let mut detail = detail_with_artifacts(&[]);
        detail.id = "browser-proof-report".to_owned();
        detail.tags = vec!["self-hosted".to_owned()];
        detail.covers = vec!["effigy.demo.browser".to_owned()];
        detail.active_terminal_session = super::DemoActiveTerminalSession {
            available: true,
            state: "running".to_owned(),
            attempt_id: Some("demo-123".to_owned()),
            runtime_backend: super::DemoRuntimeBackend {
                kind: "run".to_owned(),
                label: "run-backed".to_owned(),
                flattened_projection: false,
                projection_shape: super::DemoRuntimeProjectionShape {
                    kind: "single-terminal".to_owned(),
                    live_terminal_eligible: true,
                    projected_multi_process: false,
                    managed_process_count: None,
                },
                projected_process_summary: super::DemoRuntimeProjectedProcessSummary::default(),
                projected_output_provenance: super::DemoRuntimeProjectedOutputProvenance::default(),
                capabilities: vec![
                    "active-terminal-session".to_owned(),
                    "live-terminal-output".to_owned(),
                    "stop".to_owned(),
                ],
            },
            transport: "stream".to_owned(),
            pty: false,
            supports_input_forwarding: false,
            input_forwarding_reason: Some(
                "Input forwarding is not available for this active demo.".to_owned(),
            ),
            terminal_size: super::DemoTerminalSize {
                cols: Some(80),
                rows: Some(24),
            },
            resize: super::DemoTerminalResize {
                available: false,
                ..Default::default()
            },
            stdout_log_path: Some(".effigy/demo/logs/demo-123.stdout.log".to_owned()),
            stderr_log_path: Some(".effigy/demo/logs/demo-123.stderr.log".to_owned()),
            output_available: true,
            recent_output: super::DemoTerminalRecentOutput {
                stdout_lines: vec!["boot".to_owned(), "serve".to_owned()],
                stderr_lines: vec!["warn".to_owned()],
            },
            ..Default::default()
        };

        let repo_root = std::env::temp_dir().join(format!(
            "effigy-demo-browser-terminal-view-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(repo_root.join(".effigy/demo/logs"));
        std::fs::write(
            repo_root.join(".effigy/demo/logs/demo-123.stdout.log"),
            "boot\nserve-live\n",
        )
        .expect("write stdout log");
        std::fs::write(
            repo_root.join(".effigy/demo/logs/demo-123.stderr.log"),
            "warn-live\n",
        )
        .expect("write stderr log");

        let rendered = terminal_detail_render(&repo_root, &detail, None, true)
            .lines
            .into_iter()
            .map(|line: Line<'static>| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        let _ = std::fs::remove_dir_all(&repo_root);

        assert!(rendered.contains("source: live terminal"));
        assert!(rendered.contains("transport: stream"));
        assert!(rendered.contains("input: Input forwarding is not available for this active demo."));
        assert!(rendered.contains("boot"));
        assert!(rendered.contains("serve-live"));
        assert!(rendered.contains("stderr: recent lines"));
        assert!(rendered.contains("warn-live"));
        assert!(!rendered.contains("tags:"));
        assert!(!rendered.contains("covers:"));
    }

    #[test]
    fn browser_terminal_view_reports_unavailable_session_honestly() {
        let detail = detail_with_artifacts(&[]);

        let rendered = terminal_detail_render(Path::new("/tmp/demo-repo"), &detail, None, true)
            .lines
            .into_iter()
            .map(|line: Line<'static>| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("No active or recorded terminal output is available."));
    }

    #[test]
    fn browser_terminal_view_falls_back_to_latest_attempt_output_when_session_is_unavailable() {
        let mut detail = detail_with_artifacts(&[]);
        detail.latest_attempt.stdout_log_path =
            Some(".effigy/demo/logs/demo-latest.stdout.log".to_owned());
        detail.latest_attempt.stderr_log_path =
            Some(".effigy/demo/logs/demo-latest.stderr.log".to_owned());
        detail.latest_attempt.output_available = true;

        let repo_root = std::env::temp_dir().join(format!(
            "effigy-demo-browser-latest-terminal-view-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(repo_root.join(".effigy/demo/logs"));
        std::fs::write(
            repo_root.join(".effigy/demo/logs/demo-latest.stdout.log"),
            "latest-out\n",
        )
        .expect("write stdout log");
        std::fs::write(
            repo_root.join(".effigy/demo/logs/demo-latest.stderr.log"),
            "latest-err\n",
        )
        .expect("write stderr log");

        let rendered = terminal_detail_render(&repo_root, &detail, None, true)
            .lines
            .into_iter()
            .map(|line: Line<'static>| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        let _ = std::fs::remove_dir_all(&repo_root);

        assert!(!rendered.contains("No active terminal session is available for this demo."));
        assert!(rendered.contains("source: latest attempt logs"));
        assert!(rendered.contains("latest-out"));
        assert!(rendered.contains("latest-err"));
    }

    #[test]
    fn browser_terminal_view_falls_back_to_inspect_snapshot_when_logs_are_missing() {
        let mut detail = detail_with_artifacts(&[]);
        detail.active_terminal_session = super::DemoActiveTerminalSession {
            available: true,
            state: "running".to_owned(),
            attempt_id: Some("demo-123".to_owned()),
            runtime_backend: super::DemoRuntimeBackend {
                kind: "run".to_owned(),
                label: "run-backed".to_owned(),
                flattened_projection: false,
                projection_shape: super::DemoRuntimeProjectionShape {
                    kind: "single-terminal".to_owned(),
                    live_terminal_eligible: true,
                    projected_multi_process: false,
                    managed_process_count: None,
                },
                projected_process_summary: super::DemoRuntimeProjectedProcessSummary::default(),
                projected_output_provenance: super::DemoRuntimeProjectedOutputProvenance::default(),
                capabilities: vec![
                    "active-terminal-session".to_owned(),
                    "live-terminal-output".to_owned(),
                    "stop".to_owned(),
                    "pty".to_owned(),
                ],
            },
            transport: "pty".to_owned(),
            pty: true,
            supports_input_forwarding: false,
            input_forwarding_reason: Some(
                "Input forwarding is not available for this active demo.".to_owned(),
            ),
            terminal_size: super::DemoTerminalSize {
                cols: Some(120),
                rows: Some(32),
            },
            resize: super::DemoTerminalResize {
                available: false,
                ..Default::default()
            },
            stdout_log_path: Some(".effigy/demo/logs/missing.stdout.log".to_owned()),
            output_available: true,
            recent_output: super::DemoTerminalRecentOutput {
                stdout_lines: vec!["snapshot-line".to_owned()],
                stderr_lines: vec![],
            },
            ..Default::default()
        };

        let rendered = terminal_detail_render(Path::new("/tmp/demo-repo"), &detail, None, true)
            .lines
            .into_iter()
            .map(|line: Line<'static>| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("source: inspect snapshot"));
        assert!(rendered.contains("snapshot-line"));
    }

    #[test]
    fn browser_history_view_renders_selected_attempt_details() {
        let mut detail = detail_with_artifacts(&[]);
        detail.tags = vec!["self-hosted".to_owned()];
        let history = DemoHistoryPayload {
            schema: String::new(),
            schema_version: 1,
            ok: true,
            repo_root: String::new(),
            query: JsonValue::Null,
            demo: Default::default(),
            active_attempt: Default::default(),
            latest_attempt: Default::default(),
            attempt_history: DemoHistoryAttemptHistoryPayload {
                path: None,
                stored_count: 1,
                filtered_count: 1,
                displayed_count: 1,
                count: 1,
                limit: None,
                outcome: None,
                parse_error: None,
                attempts: vec![DemoHistoryAttempt {
                    ordinal: 1,
                    attempt_id: "demo-123".to_owned(),
                    recorded_at_epoch_ms: 1,
                    outcome: "failed".to_owned(),
                    summary: Some("Proof artifact was missing.".to_owned()),
                    receipt_path: Some(".effigy/demo/history/demo-123.json".to_owned()),
                    artifacts: vec![".effigy/demo/artifacts/report.html".to_owned()],
                    stdout_log_path: Some(".effigy/demo/logs/demo-123.stdout.log".to_owned()),
                    stderr_log_path: Some(".effigy/demo/logs/demo-123.stderr.log".to_owned()),
                    exit_code: Some(1),
                }],
            },
            selected_attempt: None,
        };

        let rendered = history_detail_render(
            &detail,
            Some(&history),
            Some(DetailSelectableItem::HistoryAttempt(1)),
            true,
        )
        .lines
        .into_iter()
        .map(|line: Line<'static>| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

        assert!(rendered.contains("Refresh history"));
        assert!(rendered.contains("#01"));
        assert!(rendered.contains("Proof artifact was missing."));
        assert!(rendered.contains("receipt:\n  .effigy/demo/history/demo-123.json"));
        assert!(rendered.contains("stdout:\n  .effigy/demo/logs/demo-123.stdout.log"));
        assert!(rendered.contains("stderr:\n  .effigy/demo/logs/demo-123.stderr.log"));
        assert!(rendered.contains("artifacts:\n  .effigy/demo/artifacts/report.html"));
        assert!(!rendered.contains("tags:"));
        assert!(!rendered.contains("Retained attempts for"));
    }

    #[test]
    fn browser_escape_returns_to_overview_tab_before_exiting() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        app.detail = Some(detail_with_artifacts(&[]));
        app.selected_demo_id = Some("demo".to_owned());
        app.detail_tab = DetailTab::History;
        app.history = Some(DemoHistoryPayload {
            schema: String::new(),
            schema_version: 1,
            ok: true,
            repo_root: String::new(),
            query: JsonValue::Null,
            demo: Default::default(),
            active_attempt: Default::default(),
            latest_attempt: Default::default(),
            attempt_history: DemoHistoryAttemptHistoryPayload {
                path: None,
                stored_count: 0,
                filtered_count: 0,
                displayed_count: 0,
                count: 0,
                limit: None,
                outcome: None,
                parse_error: None,
                attempts: vec![],
            },
            selected_attempt: None,
        });

        let should_exit = app
            .handle_key_with(
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                unexpected_invoke,
            )
            .expect("escape should succeed");

        assert!(!should_exit);
        assert!(matches!(app.detail_tab, DetailTab::Overview));
        assert_eq!(app.footer_message, "Viewing Overview tab.");
    }

    #[test]
    fn browser_tab_key_switches_between_list_and_detail_panels() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        app.detail = Some(detail_with_artifacts(&[]));
        app.selected_demo_id = Some("demo".to_owned());
        assert!(matches!(app.focus, BrowserFocus::List));

        app.handle_key_with(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            unexpected_invoke,
        )
        .expect("tab should succeed");
        assert!(matches!(app.focus, BrowserFocus::Detail));

        app.handle_key_with(
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
            unexpected_invoke,
        )
        .expect("shift-tab should succeed");
        assert!(matches!(app.focus, BrowserFocus::List));
    }

    #[test]
    fn browser_arrow_keys_switch_demo_views_inside_detail_panel() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        app.detail = Some(detail_with_artifacts(&[]));
        app.selected_demo_id = Some("demo".to_owned());
        app.focus = BrowserFocus::Detail;
        app.detail_tab = DetailTab::Terminal;

        app.handle_key_with(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            unexpected_invoke,
        )
        .expect("right should succeed");
        assert!(matches!(app.detail_tab, DetailTab::Artifacts));

        app.handle_key_with(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            unexpected_invoke,
        )
        .expect("left should succeed");
        assert!(matches!(app.detail_tab, DetailTab::Terminal));
    }

    #[test]
    fn browser_terminal_key_input_maps_terminal_controls() {
        assert_eq!(
            browser_terminal_key_input(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some("\n".to_owned())
        );
        assert_eq!(
            browser_terminal_key_input(&KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            Some("\u{1b}[D".to_owned())
        );
        assert_eq!(
            browser_terminal_key_input(&KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some("\u{3}".to_owned())
        );
    }

    #[test]
    fn browser_terminal_enter_toggles_input_mode_when_supported() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        let mut detail = detail_with_artifacts(&[]);
        detail.active_terminal_session.available = true;
        detail.active_terminal_session.supports_input_forwarding = true;
        detail.active_terminal_session.input_forwarding_reason = None;
        app.detail = Some(detail);
        app.selected_demo_id = Some("demo".to_owned());
        app.focus = BrowserFocus::Detail;
        app.detail_tab = DetailTab::Terminal;

        app.handle_enter_key_with(unexpected_invoke)
            .expect("enter should enable input mode");
        assert!(app.terminal_input_mode);

        app.handle_key_with(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            unexpected_invoke,
        )
        .expect("escape should leave input mode");
        assert!(!app.terminal_input_mode);
    }

    #[test]
    fn live_browser_run_keeps_list_focus_when_started_from_list_panel() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        let mut detail = detail_with_artifacts(&[]);
        detail.mode = "interactive".to_owned();
        detail.runtime_backend.kind = "run".to_owned();
        detail.runtime_backend.capabilities = vec!["browser-live-attach".to_owned()];
        detail.runtime_backend.projection_shape = super::DemoRuntimeProjectionShape {
            kind: "single-terminal".to_owned(),
            live_terminal_eligible: true,
            projected_multi_process: false,
            managed_process_count: None,
        };
        app.detail = Some(detail);
        app.selected_demo_id = Some("demo".to_owned());
        app.focus = BrowserFocus::List;
        app.detail_tab = DetailTab::Overview;

        app.dispatch_run_or_rerun_with(success_invoke)
            .expect("live browser run should queue");

        assert!(matches!(app.focus, BrowserFocus::List));
        assert!(matches!(app.detail_tab, DetailTab::Terminal));
        assert!(app.pending_live_terminal_launch.is_some());
    }

    #[test]
    fn run_backed_interactive_demo_prefers_live_browser_terminal() {
        let mut detail = detail_with_artifacts(&[]);
        detail.mode = "interactive".to_owned();
        detail.runtime_backend.kind = "run".to_owned();
        detail.runtime_backend.capabilities = vec!["browser-live-attach".to_owned()];
        detail.runtime_backend.projection_shape = super::DemoRuntimeProjectionShape {
            kind: "single-terminal".to_owned(),
            live_terminal_eligible: true,
            projected_multi_process: false,
            managed_process_count: None,
        };

        assert!(detail_prefers_live_browser_terminal(
            &detail,
            &crate::DemoSubcommand::Run {
                demo_id: detail.id.clone()
            }
        ));
        assert!(detail_prefers_live_browser_terminal(
            &detail,
            &crate::DemoSubcommand::Rerun {
                demo_id: detail.id.clone()
            }
        ));
    }

    #[test]
    fn concurrent_runner_single_process_demo_prefers_live_browser_terminal() {
        let mut detail = detail_with_artifacts(&[]);
        detail.mode = "interactive".to_owned();
        detail.runtime_backend.kind = "concurrent-runner".to_owned();
        detail.runtime_backend.capabilities = vec!["browser-live-attach".to_owned()];
        detail.runtime_backend.projection_shape = super::DemoRuntimeProjectionShape {
            kind: "single-terminal".to_owned(),
            live_terminal_eligible: true,
            projected_multi_process: false,
            managed_process_count: Some(1),
        };

        assert!(detail_prefers_live_browser_terminal(
            &detail,
            &crate::DemoSubcommand::Run {
                demo_id: detail.id.clone()
            }
        ));
    }

    #[test]
    fn concurrent_runner_without_live_attach_capability_does_not_prefer_live_browser_terminal() {
        let mut detail = detail_with_artifacts(&[]);
        detail.mode = "interactive".to_owned();
        detail.runtime_backend.kind = "concurrent-runner".to_owned();
        detail.runtime_backend.projection_shape = super::DemoRuntimeProjectionShape {
            kind: "projected-multi-process".to_owned(),
            live_terminal_eligible: false,
            projected_multi_process: true,
            managed_process_count: Some(2),
        };

        assert!(!detail_prefers_live_browser_terminal(
            &detail,
            &crate::DemoSubcommand::Run {
                demo_id: detail.id.clone()
            }
        ));
    }

    #[test]
    fn browser_terminal_up_down_scroll_when_detail_panel_is_active() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        app.detail = Some(detail_with_artifacts(&[]));
        app.selected_demo_id = Some("demo".to_owned());
        app.focus = BrowserFocus::Detail;
        app.detail_tab = DetailTab::Terminal;

        app.handle_down_key();
        app.handle_down_key();
        assert_eq!(app.terminal_scroll_offset, 2);

        app.handle_up_key();
        assert_eq!(app.terminal_scroll_offset, 1);
    }

    #[test]
    fn browser_live_terminal_sanitizer_strips_literal_ctrl_d_marker_only() {
        let mut transcript = b"\x1b[2Jhello".to_vec();
        transcript.extend_from_slice(&sanitize_live_terminal_bytes(b"^D\x08\x08world"));

        assert_eq!(String::from_utf8_lossy(&transcript), "\u{1b}[2Jhelloworld");
    }

    #[test]
    fn browser_live_terminal_sanitizer_normalizes_lf_to_crlf() {
        let sanitized = sanitize_live_terminal_bytes(b"one\ntwo\r\nthree\n");

        assert_eq!(sanitized, b"one\r\ntwo\r\nthree\r\n");
    }

    #[test]
    fn browser_live_terminal_buffers_split_utf8_border_bytes() {
        let mut carry = Vec::new();
        let top_left = "╭".as_bytes();
        let first = take_complete_terminal_bytes(&mut carry, &top_left[..1]);
        let second = take_complete_terminal_bytes(&mut carry, &top_left[1..]);
        let third = take_complete_terminal_bytes(&mut carry, "──╮\n".as_bytes());

        let mut parser = vt100::Parser::new(24, 72, DEMO_BROWSER_TERMINAL_PARSER_SCROLLBACK);
        parser.process(&first);
        parser.process(&second);
        parser.process(&third);

        let rendered = browser_vt_lines(&mut parser, 72, 4, 0)
            .0
            .into_iter()
            .map(|line: Line<'static>| line.to_string())
            .collect::<Vec<_>>();

        assert!(rendered.iter().any(|line| line.contains("╭──╮")));
    }

    #[test]
    fn browser_live_terminal_env_forces_color_and_size() {
        let env = browser_live_terminal_env(Some((120, 33)));

        assert!(env.contains(&("EFFIGY_COLOR".to_owned(), "always".to_owned())));
        assert!(env.contains(&(DEMO_BROWSER_TERMINAL_COLS_ENV.to_owned(), "120".to_owned())));
        assert!(env.contains(&(DEMO_BROWSER_TERMINAL_ROWS_ENV.to_owned(), "33".to_owned())));
    }

    #[test]
    fn browser_live_terminal_env_forces_color_without_size() {
        let env = browser_live_terminal_env(None);

        assert_eq!(env, vec![("EFFIGY_COLOR".to_owned(), "always".to_owned())]);
    }

    #[test]
    fn browser_artifacts_tab_renders_artifact_entries() {
        let mut detail = detail_with_artifacts(&["one", "two"]);
        detail.tags = vec!["self-hosted".to_owned()];
        detail.covers = vec!["effigy.demo.browser".to_owned()];
        let rendered =
            artifacts_detail_render(&detail, Some(DetailSelectableItem::Artifact(1)), true)
                .lines
                .into_iter()
                .map(|line: Line<'static>| line.to_string())
                .collect::<Vec<_>>()
                .join("\n");

        assert!(rendered.contains("one"));
        assert!(rendered.contains("two"));
        assert!(!rendered.contains("Artifacts"));
        assert!(!rendered.contains("↑/↓ selects artifacts"));
        assert!(!rendered.contains("Enter opens selection"));
        assert!(!rendered.contains("tags:"));
        assert!(!rendered.contains("covers:"));
        assert!(!rendered.contains("Recorded artifacts for"));
    }

    #[test]
    fn browser_tab_renders_do_not_repeat_title_chrome() {
        let mut detail = detail_with_artifacts(&["one"]);
        detail.title = "Browser Proof Report".to_owned();

        let history_rendered = history_detail_render(&detail, None, None, true)
            .lines
            .into_iter()
            .map(|line: Line<'static>| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!history_rendered.contains("Browser Proof Report"));
        assert!(!history_rendered.contains("History View"));

        let terminal_rendered =
            terminal_detail_render(Path::new("/tmp/demo-repo"), &detail, None, true)
                .lines
                .into_iter()
                .map(|line: Line<'static>| line.to_string())
                .collect::<Vec<_>>()
                .join("\n");
        assert!(!terminal_rendered.contains("Browser Proof Report"));
        assert!(!terminal_rendered.contains("Terminal View"));

        let artifacts_rendered = artifacts_detail_render(&detail, None, true)
            .lines
            .into_iter()
            .map(|line: Line<'static>| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!artifacts_rendered.contains("Browser Proof Report"));
        assert!(!artifacts_rendered.contains("Artifacts View"));
    }

    #[test]
    fn browser_escape_exits_from_overview_root_view() {
        let mut app = DemoBrowserApp::new(PathBuf::from("/tmp/demo-repo"), None);
        app.detail = Some(detail_with_artifacts(&[]));
        app.selected_demo_id = Some("demo".to_owned());

        let should_exit = app
            .handle_key_with(
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                unexpected_invoke,
            )
            .expect("escape should succeed");

        assert!(should_exit);
    }
}
