use super::{PulseReport, ResolutionMode, Task, TaskContext, TaskError};
use std::fs;
use std::path::{Path, PathBuf};

pub struct PulseTask;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PulseCollected {
    repo_path: PathBuf,
    repo: String,
    marker_hits: Vec<String>,
    resolution_mode: ResolutionMode,
    resolution_evidence: Vec<String>,
    resolution_warnings: Vec<String>,
    package_scripts: Vec<String>,
    package_parse_warning: Option<String>,
    subrepo_candidates: Vec<String>,
    has_effigy_link: bool,
    is_coordination_repo: bool,
    has_updated_dates_script: bool,
}

impl PulseTask {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PulseTask {
    fn default() -> Self {
        Self::new()
    }
}

impl Task for PulseTask {
    type Collected = PulseCollected;
    type Evaluated = PulseReport;

    fn id(&self) -> &'static str {
        "pulse"
    }

    fn collect(&self, ctx: &TaskContext) -> Result<Self::Collected, TaskError> {
        let repo_path = ctx.target_repo.clone();
        let mut marker_hits: Vec<String> = Vec::new();
        for marker in ["package.json", "Cargo.toml", ".git"] {
            let marker_path = repo_path.join(marker);
            if marker_path.exists() {
                marker_hits.push(marker.to_owned());
            }
        }

        let (package_scripts, package_parse_warning) = read_package_scripts(&repo_path);
        let subrepo_candidates = find_subrepo_candidates(&repo_path);
        let has_effigy_link = repo_path.join("effigy").exists();
        let is_coordination_repo = ["strategy", "experiments", "projects"]
            .iter()
            .all(|dir| repo_path.join(dir).is_dir());
        let has_updated_dates_script = repo_path.join("scripts/check-updated-dates.sh").exists();

        Ok(PulseCollected {
            repo_path: repo_path.clone(),
            repo: repo_path.display().to_string(),
            marker_hits,
            resolution_mode: ctx.resolution_mode,
            resolution_evidence: ctx.resolution_evidence.clone(),
            resolution_warnings: ctx.resolution_warnings.clone(),
            package_scripts,
            package_parse_warning,
            subrepo_candidates,
            has_effigy_link,
            is_coordination_repo,
            has_updated_dates_script,
        })
    }

    fn evaluate(&self, collected: Self::Collected) -> Result<Self::Evaluated, TaskError> {
        let mut evidence: Vec<String> = Vec::new();
        let mut risk: Vec<String> = Vec::new();
        let mut next_action: Vec<String> = Vec::new();

        if collected.marker_hits.is_empty() {
            evidence.push("No root markers found at target path".to_owned());
        } else {
            evidence.push(format!(
                "Detected root markers: {}",
                collected.marker_hits.join(", ")
            ));
        }

        if collected.package_scripts.is_empty() {
            evidence.push("No root package.json scripts detected.".to_owned());
        } else {
            evidence.push(format!(
                "Root package.json scripts: {}",
                collected.package_scripts.join(", ")
            ));
        }

        if !collected.subrepo_candidates.is_empty() {
            evidence.push(format!(
                "Detected repo/workspace candidates at root: {}",
                collected.subrepo_candidates.join(", ")
            ));
        }
        evidence.push(format!(
            "effigy link present: {}",
            if collected.has_effigy_link {
                "yes"
            } else {
                "no"
            }
        ));

        if collected.is_coordination_repo {
            evidence.push(
                "Detected coordination-style markdown repo layout (strategy/experiments/projects)."
                    .to_owned(),
            );
            evidence.push(format!(
                "check-updated-dates script present: {}",
                if collected.has_updated_dates_script {
                    "yes"
                } else {
                    "no"
                }
            ));
        }

        evidence.push(format!(
            "Resolution mode: {}",
            match collected.resolution_mode {
                ResolutionMode::Explicit => "explicit (--repo)",
                ResolutionMode::AutoNearest => "auto (nearest root)",
                ResolutionMode::AutoPromoted => "auto (promoted workspace root)",
            }
        ));

        for &line in &ctx_lines_from_mode(collected.resolution_mode) {
            evidence.push(line.to_owned());
        }
        for item in collected.resolution_evidence {
            evidence.push(format!("Root evidence: {item}"));
        }

        if collected.marker_hits.is_empty() {
            risk.push("Target may not be a project root; pulse signal quality is low.".to_owned());
        }

        let has_health_script = collected
            .package_scripts
            .iter()
            .any(|script| script == "health:workspace" || script == "health");
        let should_expect_task_surface = should_expect_root_task_surface(
            &collected.marker_hits,
            &collected.package_scripts,
            collected.has_effigy_link,
        );
        if !has_health_script && should_expect_task_surface {
            if collected.package_scripts.is_empty() && collected.subrepo_candidates.len() >= 3 {
                risk.push(
                    "Workspace appears to have multiple subrepos but no root task surface."
                        .to_owned(),
                );
                next_action.push(format!(
                    "Add `{}` with scripts `list:repos` and `health:workspace`, then run `effigy pulse --repo {}`.",
                    collected.repo_path.join("package.json").display(),
                    collected.repo
                ));
            } else if !collected.package_scripts.is_empty() {
                risk.push("Root package.json lacks a canonical health-check command.".to_owned());
                next_action.push(format!(
                    "Update `{}` to add `health:workspace` (or `health`) script and route it to repo smoke checks.",
                    collected.repo_path.join("package.json").display()
                ));
            }
        }

        if collected.is_coordination_repo && !collected.has_updated_dates_script {
            risk.push(
                "Coordination repo is missing an automated `updated:` staleness guard.".to_owned(),
            );
            next_action.push(format!(
                "Add `{}` and wire a root command (`check:updated-dates`) to enforce markdown `updated:` freshness.",
                collected.repo_path.join("scripts/check-updated-dates.sh").display()
            ));
        }

        if let Some(parse_warning) = collected.package_parse_warning {
            risk.push(parse_warning);
        }

        for warning in collected.resolution_warnings {
            risk.push(format!("Root warning: {warning}"));
        }

        if next_action.is_empty() {
            next_action
                .push("No high-priority structural gaps detected by pulse v0 signals.".to_owned());
        }

        Ok(PulseReport {
            repo: collected.repo,
            evidence,
            risk,
            next_action,
            owner: "platform".to_owned(),
            eta: "phase-22".to_owned(),
        })
    }

    fn render(&self, report: Self::Evaluated) -> Result<String, TaskError> {
        let mut out = String::new();
        out.push_str("# Pulse Report\n\n");
        out.push_str(&format!("- repo: {}\n", report.repo));
        out.push_str("- evidence:\n");
        for item in report.evidence {
            out.push_str(&format!("  - {}\n", item));
        }
        out.push_str("- risk:\n");
        for item in report.risk {
            out.push_str(&format!("  - {}\n", item));
        }
        out.push_str("- next-action:\n");
        for item in report.next_action {
            out.push_str(&format!("  - {}\n", item));
        }
        out.push_str(&format!("- owner: {}\n", report.owner));
        out.push_str(&format!("- eta: {}\n", report.eta));
        Ok(out)
    }
}

fn ctx_lines_from_mode(mode: ResolutionMode) -> [&'static str; 1] {
    match mode {
        ResolutionMode::Explicit => ["Root resolution source: explicit override"],
        ResolutionMode::AutoNearest => ["Root resolution source: nearest marker candidate"],
        ResolutionMode::AutoPromoted => ["Root resolution source: parent workspace promotion"],
    }
}

fn should_expect_root_task_surface(
    marker_hits: &[String],
    package_scripts: &[String],
    has_effigy_link: bool,
) -> bool {
    if !package_scripts.is_empty() || has_effigy_link {
        return true;
    }
    marker_hits
        .iter()
        .any(|m| m == "package.json" || m == "Cargo.toml")
}

fn read_package_scripts(repo_root: &Path) -> (Vec<String>, Option<String>) {
    let package_path = repo_root.join("package.json");
    if !package_path.exists() {
        return (Vec::new(), None);
    }

    let content = match fs::read_to_string(&package_path) {
        Ok(content) => content,
        Err(err) => {
            return (
                Vec::new(),
                Some(format!(
                    "Unable to read {}: {}",
                    package_path.display(),
                    err
                )),
            )
        }
    };

    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(err) => {
            return (
                Vec::new(),
                Some(format!(
                    "Unable to parse {} as JSON: {}",
                    package_path.display(),
                    err
                )),
            )
        }
    };

    let Some(scripts) = value.get("scripts").and_then(|v| v.as_object()) else {
        return (Vec::new(), None);
    };
    let mut script_names: Vec<String> = scripts.keys().cloned().collect();
    script_names.sort();
    (script_names, None)
}

fn find_subrepo_candidates(repo_root: &Path) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    let Ok(entries) = fs::read_dir(repo_root) else {
        return candidates;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || matches!(name, "node_modules" | "target") {
            continue;
        }

        let looks_like_repo = [".git", "package.json", "Cargo.toml", "AGENTS.md"]
            .iter()
            .any(|marker| path.join(marker).exists());
        if looks_like_repo {
            candidates.push(name.to_owned());
        }
    }

    candidates.sort();
    candidates
}

#[cfg(test)]
#[path = "../tests/tasks/pulse_tests.rs"]
mod tests;
