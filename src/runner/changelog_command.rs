//! CLI command handler for `effigy changelog` subcommands.

use std::path::PathBuf;

use effigy_changelog as changelog;
use effigy_cli::{ChangelogArgs, ChangelogSubcommand};

use super::command_context::resolve_active_repo_root;
use super::error::RunnerError;

/// Execute a changelog subcommand and return the output string.
pub(super) fn run_changelog(args: ChangelogArgs) -> Result<String, RunnerError> {
    let file = resolve_changelog_file(args.repo_override.as_deref(), args.file.as_deref())?;

    match args.subcommand {
        ChangelogSubcommand::Validate => run_validate(&file, args.output_json),
        ChangelogSubcommand::Format { write } => run_format(&file, write, args.output_json),
        ChangelogSubcommand::Analyze => run_analyze(&file, args.output_json),
        ChangelogSubcommand::Extract { version } => run_extract(&file, &version, args.output_json),
    }
}

fn resolve_changelog_file(
    repo_override: Option<&std::path::Path>,
    file: Option<&std::path::Path>,
) -> Result<PathBuf, RunnerError> {
    match repo_override {
        Some(path) => {
            let resolved = resolve_active_repo_root(Some(path.to_path_buf()))?;
            let repo_root = resolved.resolved_root;
            Ok(match file {
                Some(file) if file.is_absolute() => file.to_path_buf(),
                Some(file) => repo_root.join(file),
                None => repo_root.join("CHANGELOG.md"),
            })
        }
        None => Ok(file
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("CHANGELOG.md"))),
    }
}

fn run_validate(file: &PathBuf, output_json: bool) -> Result<String, RunnerError> {
    let raw_content =
        std::fs::read_to_string(file).map_err(|e| RunnerError::task_invocation(e.to_string()))?;

    let parsed =
        changelog::parse(&raw_content).map_err(|e| RunnerError::task_invocation(e.to_string()))?;

    let diagnostics = changelog::validate(&parsed, &raw_content);

    if output_json {
        let json = build_validate_json(&diagnostics);
        if !diagnostics.is_empty() {
            return Err(RunnerError::task_invocation(json));
        }
        return Ok(json);
    }

    if diagnostics.is_empty() {
        return Ok(format!("{}: valid ✓", file.display()));
    }

    let mut output = format!("{}: {} issue(s) found\n", file.display(), diagnostics.len());
    for d in &diagnostics {
        output.push_str(&format!("\n  {d}"));
    }
    Err(RunnerError::task_invocation(output))
}

fn run_format(file: &PathBuf, write: bool, _output_json: bool) -> Result<String, RunnerError> {
    let raw_content =
        std::fs::read_to_string(file).map_err(|e| RunnerError::task_invocation(e.to_string()))?;

    let parsed =
        changelog::parse(&raw_content).map_err(|e| RunnerError::task_invocation(e.to_string()))?;

    let formatted = changelog::format(&parsed);

    if write {
        std::fs::write(file, &formatted)
            .map_err(|e| RunnerError::task_invocation(e.to_string()))?;
        Ok(format!("{}: formatted ✓", file.display()))
    } else {
        Ok(formatted)
    }
}

fn run_analyze(file: &PathBuf, output_json: bool) -> Result<String, RunnerError> {
    let raw_content =
        std::fs::read_to_string(file).map_err(|e| RunnerError::task_invocation(e.to_string()))?;

    let parsed =
        changelog::parse(&raw_content).map_err(|e| RunnerError::task_invocation(e.to_string()))?;

    let analysis = changelog::analyze(&parsed);

    if output_json {
        return Ok(build_analyze_json(&analysis));
    }

    let mut output = String::new();

    if analysis.unreleased_is_empty {
        output.push_str("Unreleased: empty\n");
    } else {
        output.push_str("Unreleased entries:\n");
        for (category, count) in &analysis.unreleased_counts {
            output.push_str(&format!("  {category}: {count}\n"));
        }
    }

    output.push_str(&format!("Suggested bump: {}\n", analysis.suggested_bump));

    if let Some(ref current) = analysis.current_version {
        output.push_str(&format!("Current version: {current}\n"));
    }
    if let Some(ref next) = analysis.next_version {
        output.push_str(&format!("Next version: {next}\n"));
    }

    Ok(output)
}

fn run_extract(file: &PathBuf, version: &str, _output_json: bool) -> Result<String, RunnerError> {
    let raw_content =
        std::fs::read_to_string(file).map_err(|e| RunnerError::task_invocation(e.to_string()))?;

    let parsed =
        changelog::parse(&raw_content).map_err(|e| RunnerError::task_invocation(e.to_string()))?;

    match changelog::extract_version(&parsed, version) {
        Some(notes) => Ok(notes),
        None => Err(RunnerError::task_invocation(format!(
            "version `{version}` not found or has no entries in {}",
            file.display()
        ))),
    }
}

fn build_validate_json(diagnostics: &[changelog::ValidationDiagnostic]) -> String {
    let mut json = String::from("{\n");
    json.push_str(&format!("  \"valid\": {},\n", diagnostics.is_empty()));
    json.push_str("  \"diagnostics\": [");

    for (i, d) in diagnostics.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            "\n    {{\"line\": {}, \"rule\": \"{}\", \"message\": {}}}",
            d.line,
            d.rule,
            json_escape_string(&d.message)
        ));
    }

    if !diagnostics.is_empty() {
        json.push('\n');
        json.push_str("  ");
    }
    json.push_str("]\n}");
    json
}

fn build_analyze_json(analysis: &changelog::Analysis) -> String {
    let mut json = String::from("{\n");

    json.push_str("  \"unreleased\": {");
    let mut first = true;
    for (category, count) in &analysis.unreleased_counts {
        if !first {
            json.push(',');
        }
        first = false;
        json.push_str(&format!("\n    \"{}\": {}", category.to_lowercase(), count));
    }
    if !analysis.unreleased_counts.is_empty() {
        json.push('\n');
        json.push_str("  ");
    }
    json.push_str("},\n");

    json.push_str(&format!(
        "  \"unreleased_is_empty\": {},\n",
        analysis.unreleased_is_empty
    ));
    json.push_str(&format!(
        "  \"suggested_bump\": \"{}\",\n",
        analysis.suggested_bump
    ));

    match &analysis.current_version {
        Some(v) => json.push_str(&format!("  \"current_version\": \"{v}\",\n")),
        None => json.push_str("  \"current_version\": null,\n"),
    }
    match &analysis.next_version {
        Some(v) => json.push_str(&format!("  \"next_version\": \"{v}\"\n")),
        None => json.push_str("  \"next_version\": null\n"),
    }

    json.push('}');
    json
}

fn json_escape_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len() + 2);
    escaped.push('"');
    for ch in s.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c => escaped.push(c),
        }
    }
    escaped.push('"');
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_repo(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "effigy-changelog-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("mkdir");
        root
    }

    #[test]
    fn changelog_repo_override_anchors_default_path_to_repo_root() {
        let repo = temp_repo("default");
        fs::write(
            repo.join("CHANGELOG.md"),
            "# Changelog\n\n## [Unreleased]\n\n### Added\n- one\n",
        )
        .expect("write changelog");

        let resolved = resolve_changelog_file(Some(repo.as_path()), None).expect("resolve");
        assert!(resolved.ends_with("CHANGELOG.md"));

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn changelog_repo_override_anchors_relative_file_to_repo_root() {
        let repo = temp_repo("relative");
        fs::create_dir_all(repo.join("notes")).expect("mkdir notes");
        fs::write(
            repo.join("notes/CHANGELOG.alt.md"),
            "# Changelog\n\n## [Unreleased]\n\n### Added\n- one\n",
        )
        .expect("write changelog");

        let resolved = resolve_changelog_file(
            Some(repo.as_path()),
            Some(std::path::Path::new("notes/CHANGELOG.alt.md")),
        )
        .expect("resolve");
        assert!(resolved.ends_with("notes/CHANGELOG.alt.md"));

        let _ = fs::remove_dir_all(repo);
    }
}
