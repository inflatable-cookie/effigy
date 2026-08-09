use chrono::Utc;
use effigy_cli::{PapercutsArgs, PapercutsSubcommand};
use effigy_papercuts::{
    add, discover, PapercutAddReport, PapercutDraft, PapercutReport, PapercutStatus, ScopeMode,
};
use std::path::{Path, PathBuf};

use super::RunnerError;

pub(in crate::runner) fn run_papercuts_with_cwd(
    args: PapercutsArgs,
    cwd: &Path,
) -> Result<String, RunnerError> {
    let scope = resolve_scope_arg(args.scope, cwd);
    match args.subcommand {
        PapercutsSubcommand::List { include_closed } => {
            let report = discover(&scope, include_closed)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            if args.output_json {
                serde_json::to_string_pretty(&report)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))
            } else {
                Ok(render_report(&report))
            }
        }
        PapercutsSubcommand::Add {
            title,
            friction,
            impact,
            possible_fix,
            surface,
        } => {
            let draft = PapercutDraft {
                title,
                friction,
                impact,
                possible_fix,
                surface,
            };
            let date = Utc::now().date_naive().format("%Y-%m-%d").to_string();
            let entry = add(&scope, &date, &draft)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            if args.output_json {
                serde_json::to_string_pretty(&PapercutAddReport::new(entry))
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))
            } else {
                Ok(format!(
                    "Added papercut `{}` to {}",
                    entry.title,
                    entry.source_path.display()
                ))
            }
        }
    }
}

fn resolve_scope_arg(scope: Option<PathBuf>, cwd: &Path) -> PathBuf {
    match scope {
        Some(scope) if scope.is_absolute() => scope,
        Some(scope) => cwd.join(scope),
        None => cwd.to_path_buf(),
    }
}

fn render_report(report: &PapercutReport) -> String {
    let mode = match report.mode {
        ScopeMode::Project => "project",
        ScopeMode::Collection => "collection",
    };
    let mut lines = vec![
        "Papercuts".to_owned(),
        format!("scope: {} ({mode})", report.scope.display()),
        format!(
            "projects: {}  files: {}  open: {}  closed: {}  diagnostics: {}",
            report.summary.projects_scanned,
            report.summary.files_found,
            report.summary.open,
            report.summary.closed,
            report.summary.diagnostics
        ),
    ];
    if report.entries.is_empty() {
        lines.push(String::new());
        lines.push("No papercuts found.".to_owned());
    } else {
        let mut project = None;
        for entry in &report.entries {
            if project != Some(entry.project.as_str()) {
                lines.push(String::new());
                lines.push(entry.project.clone());
                project = Some(&entry.project);
            }
            let status = match entry.status {
                PapercutStatus::Open => "open",
                PapercutStatus::Closed => "closed",
            };
            lines.push(format!("- [{status}] {}  {}", entry.date, entry.title));
            lines.push(format!("  Surface: {}", entry.surface));
            lines.push(format!(
                "  Source: {}:{}",
                entry.source_path.display(),
                entry.source_line
            ));
        }
    }
    if !report.diagnostics.is_empty() {
        lines.push(String::new());
        lines.push("Diagnostics".to_owned());
        for diagnostic in &report.diagnostics {
            lines.push(format!(
                "- {}:{}: {}",
                diagnostic.source_path.display(),
                diagnostic.source_line,
                diagnostic.message
            ));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn queue() -> &'static str {
        "# Papercuts\n\n## Open\n\n### [ ] A cut — 2026-08-09\n- Friction: slow\n- Impact: repeat\n- Possible fix: fix\n- Surface: docs\n"
    }

    #[test]
    fn command_uses_supplied_cwd_for_relative_collection_scope() {
        let temp = TempDir::new().unwrap();
        let collection = temp.path().join("projects");
        let project = collection.join("alpha");
        fs::create_dir_all(project.join(".git")).unwrap();
        fs::write(project.join("PAPERCUTS.md"), queue()).unwrap();
        let output = run_papercuts_with_cwd(
            PapercutsArgs {
                subcommand: PapercutsSubcommand::List {
                    include_closed: false,
                },
                scope: Some(PathBuf::from("projects")),
                output_json: true,
            },
            temp.path(),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["schema"], "effigy.papercuts.v1");
        assert_eq!(value["summary"]["open"], 1);
    }
}
