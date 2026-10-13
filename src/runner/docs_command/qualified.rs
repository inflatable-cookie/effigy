//! Selection and read declarations only. Resource authority stays in the live provider.
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use effigy_docs_policy::{collect_link_check_files, collect_markdown_local_targets};
use effigy_manifest::load_task_manifest_with_inspection;
use effigy_routing::{load_isolated_catalog, select_catalog_and_task};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::RunnerError;

const LEAVES: [&str; 3] = ["qa:docs:links", "qa:docs:paths", "qa:docs:retired"];
const SCRIPT: &str = "88d2b90964111596d8c85c86e7720a759b97107186820f0e72ed976a6f929cee";
const SKILL_MANIFEST: &str = "8582994b1558165ead59f03b3e59bcec9f82d3dc545e92dd5bf50f863b4caf00";
const MARKER: &str = "e98ea2dc89276042c30bad786150e9d08f10707795728480185f747f5dae16c7";

fn refuse(reason: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(format!("qualified docs unavailable: {reason}"))
}

fn bytes(path: &Path) -> Result<Value, RunnerError> {
    if path.symlink_metadata().map_err(refuse)?.is_symlink() {
        return Err(refuse(format!("indirect input {}", path.display())));
    }
    let data = std::fs::read(path).map_err(refuse)?;
    Ok(
        json!({"canonical": path, "bytes": data.len(), "sha256": Sha256::digest(&data).iter().map(|byte| format!("{byte:02x}")).collect::<String>()}),
    )
}

fn inside(root: &Path, path: &Path) -> Result<PathBuf, RunnerError> {
    let mut normalized = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                if normalized == root || !normalized.starts_with(root) {
                    return Err(refuse("read traverses outside source"));
                }
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    if !normalized.starts_with(root) {
        return Err(refuse(format!("read escapes source: {}", path.display())));
    }
    Ok(normalized)
}

fn inventory(
    root: &Path,
    dir: &Path,
    files: &mut Vec<Value>,
    dirs: &mut Vec<PathBuf>,
) -> Result<(), RunnerError> {
    for entry in std::fs::read_dir(dir).map_err(refuse)? {
        let path = entry.map_err(refuse)?.path();
        let stat = path.symlink_metadata().map_err(refuse)?;
        if stat.is_dir() {
            dirs.push(path.strip_prefix(root).map_err(refuse)?.to_path_buf());
            inventory(root, &path, files, dirs)?;
        } else if stat.is_file() {
            if stat.len() > 256 * 1024 * 1024 {
                return Err(refuse("source input byte budget"));
            }
            let mut input = bytes(&path)?;
            input["relative"] = json!(path.strip_prefix(root).map_err(refuse)?);
            files.push(input);
        } else {
            return Err(refuse(format!("indirect/unknown input {}", path.display())));
        }
        if files.len() + dirs.len() > 1984 {
            return Err(refuse("source inventory exceeds provider reserve"));
        }
    }
    Ok(())
}

fn literal_words(command: &str) -> Result<Vec<String>, RunnerError> {
    let words = command
        .split_ascii_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if words.is_empty()
        || words.iter().any(|s| {
            !s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_./:-".contains(&c))
        })
    {
        return Err(refuse(
            "caller shell text, operators or unsupported literal argv",
        ));
    }
    Ok(words)
}

fn task_run<'a>(value: &'a toml::Value, name: &str) -> Result<&'a toml::Value, RunnerError> {
    let task = value
        .get("tasks")
        .and_then(|v| v.get(name))
        .ok_or_else(|| refuse(format!("missing task {name}")))?;
    if let Some(table) = task.as_table() {
        if table.keys().any(|k| k != "run" && k != "run_in")
            || table
                .get("run_in")
                .is_some_and(|v| v.as_str() != Some("host"))
        {
            return Err(refuse(format!("unsupported task configuration {name}")));
        }
        table.get("run").ok_or_else(|| refuse("missing run"))
    } else {
        Ok(task)
    }
}

// The ordinary composition owner is authoritative. Before invoking it, this
// constrained surface admits only literal, source-local include reads.
fn guard_includes(
    root: &Path,
    file: &Path,
    seen: &mut BTreeSet<PathBuf>,
) -> Result<(), RunnerError> {
    let file = inside(root, file)?;
    if !seen.insert(file.clone()) {
        return Ok(());
    }
    let value: toml::Value =
        toml::from_str(&std::fs::read_to_string(&file).map_err(refuse)?).map_err(refuse)?;
    if let Some(includes) = value.get("manifest").and_then(|v| v.get("include")) {
        for include in includes
            .as_array()
            .ok_or_else(|| refuse("literal include list required"))?
        {
            let literal = include
                .as_str()
                .ok_or_else(|| refuse("optional/override include shape"))?;
            if Path::new(literal).is_absolute() {
                return Err(refuse("absolute includes cannot bind private execution"));
            }
            if literal_words(literal)?.len() != 1 {
                return Err(refuse("literal include path required"));
            }
            let parent = file.parent().ok_or_else(|| refuse("include parent"))?;
            guard_includes(root, &parent.join(literal), seen)?;
        }
    }
    Ok(())
}

pub(super) fn resolve(root: &Path) -> Result<String, RunnerError> {
    let root = std::fs::canonicalize(root).map_err(refuse)?;
    if root.join("effigy.local.toml").exists() {
        return Err(refuse("local manifest overlay"));
    }
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    inventory(&root, &root, &mut files, &mut dirs)?;
    files.sort_by_key(|v| v["relative"].as_str().unwrap_or_default().to_owned());
    dirs.sort();
    let manifest = root.join("effigy.toml");
    guard_includes(&root, &manifest, &mut BTreeSet::new())?;
    let loaded = load_task_manifest_with_inspection(&manifest).map_err(refuse)?;
    let table = loaded
        .effective_value
        .as_table()
        .ok_or_else(|| refuse("manifest table"))?;
    if table
        .keys()
        .any(|k| !["manifest", "catalog", "tasks"].contains(&k.as_str()))
        || loaded.bundle_root.is_some()
        || !loaded.overridden_paths.is_empty()
    {
        return Err(refuse(
            "unsupported defaults, environment, bundle or policy configuration",
        ));
    }
    let manifests = loaded
        .evaluation_order
        .iter()
        .map(|p| {
            let path = std::fs::canonicalize(p).map_err(refuse)?;
            inside(&root, &path)?;
            bytes(&path)
        })
        .collect::<Result<Vec<_>, RunnerError>>()?;
    let catalog = load_isolated_catalog(&root, &manifest).map_err(refuse)?;
    let selector = effigy_tasks::parse_task_selector("qa:docs").map_err(refuse)?;
    select_catalog_and_task(&selector, std::slice::from_ref(&catalog), &root).map_err(refuse)?;
    let sequence = task_run(&loaded.effective_value, "qa:docs")?
        .as_array()
        .ok_or_else(|| refuse("qa:docs must be the ordered three task references"))?;
    if sequence.len() != LEAVES.len()
        || sequence.iter().zip(LEAVES).any(|(v, name)| {
            v.as_table().is_none_or(|t| {
                t.len() != 1 || t.get("task").and_then(toml::Value::as_str) != Some(name)
            })
        })
    {
        return Err(refuse("qa:docs is not the exact ordered three-leaf recipe"));
    }
    let mut leaves = Vec::new();
    for name in LEAVES {
        let selector = effigy_tasks::parse_task_selector(name).map_err(refuse)?;
        select_catalog_and_task(&selector, std::slice::from_ref(&catalog), &root)
            .map_err(refuse)?;
        let command = task_run(&loaded.effective_value, name)?
            .as_str()
            .ok_or_else(|| refuse("unsupported nested composition"))?;
        let argv = literal_words(command)?;
        match name {
            "qa:docs:links" if argv == ["effigy", "docs", "check", "links"] => {}
            "qa:docs:paths"
                if argv.starts_with(&[
                    "effigy".into(),
                    "docs".into(),
                    "check".into(),
                    "paths".into(),
                ]) && argv.len() > 4 =>
            {
                for p in argv.iter().skip(4) {
                    if p.starts_with('-') || Path::new(p).is_absolute() {
                        return Err(refuse("paths passthrough"));
                    }
                    inside(&root, &root.join(p))?;
                }
            }
            "qa:docs:retired"
                if argv == ["effigy", "skill", "run", "northstar/retired-concepts"] => {}
            _ => return Err(refuse(format!("unsupported leaf argv {name}"))),
        }
        let parsed = effigy_cli::parse_command(argv.iter().skip(1).cloned()).map_err(refuse)?;
        let ast = match parsed {
            effigy_cli::Command::Docs(effigy_cli::DocsArgs {
                subcommand: effigy_cli::DocsSubcommand::Check { kind, paths, .. },
                ..
            }) => json!({"surface": "docs-check", "kind": kind.as_str(), "paths": paths}),
            effigy_cli::Command::Skill(_) => {
                json!({"surface": "named-skill", "selector": "northstar/retired-concepts"})
            }
            _ => return Err(refuse("unsupported parsed leaf AST")),
        };
        leaves.push(json!({"task": name, "argv": argv, "ast": ast}));
    }
    let mut existence = Vec::new();
    for file in collect_link_check_files(&root, &[]) {
        for (target, path) in collect_markdown_local_targets(&file).map_err(|e| refuse(e.reason))? {
            if Path::new(target.split('#').next().unwrap_or_default().trim()).is_absolute() {
                return Err(refuse("absolute links cannot bind private execution"));
            }
            inside(&root, &path)?;
            existence.push(json!({"path": path, "exists": path.exists()}));
        }
    }
    let source =
        crate::runner::skill_command::resolve_named_skill_source("northstar/retired-concepts")?;
    if source.source_root != Path::new("/Users/tom/.agents/skills/northstar") {
        return Err(refuse(
            "named source shadow or unsupported canonical installation",
        ));
    }
    let namespace =
        crate::runner::skill_command::named_skill_discovery_reads("northstar/retired-concepts")?
            .into_iter()
            .map(|path| {
                let exists = path.try_exists().map_err(refuse)?;
                let canonical = if exists {
                    Some(std::fs::canonicalize(&path).map_err(refuse)?)
                } else {
                    None
                };
                Ok(json!({"path": path, "exists": exists, "canonical": canonical}))
            })
            .collect::<Result<Vec<_>, RunnerError>>()?;
    let mut assets = Vec::new();
    for (name, digest) in [
        ("SKILL.md", MARKER),
        ("effigy.toml", SKILL_MANIFEST),
        ("scripts/retired-concepts.ts", SCRIPT),
    ] {
        let mut input = bytes(&source.source_root.join(name))?;
        if input["sha256"] != digest {
            return Err(refuse(format!("named asset drift {name}")));
        }
        input["relative"] = json!(name);
        assets.push(input);
    }
    if source.source_root.join("effigy.local.toml").exists() {
        return Err(refuse("named source overlay"));
    }
    let skill =
        load_isolated_catalog(&source.source_root, &source.manifest_path).map_err(refuse)?;
    let selector =
        effigy_tasks::parse_task_selector("northstar/retired-concepts").map_err(refuse)?;
    select_catalog_and_task(&selector, std::slice::from_ref(&skill), &source.source_root)
        .map_err(refuse)?;
    let skill_loaded = load_task_manifest_with_inspection(&source.manifest_path).map_err(refuse)?;
    if task_run(&skill_loaded.effective_value, "retired-concepts")?.as_str()
        != Some("bun run {skill}/scripts/retired-concepts.ts {args}")
    {
        return Err(refuse("named recipe drift"));
    }
    let binary = std::fs::canonicalize(std::env::current_exe().map_err(refuse)?).map_err(refuse)?;
    serde_json::to_string(&json!({
        "schema": "effigy.docs.qualified.resolution.v1", "version": 1,
        "target": root, "selector": "qa:docs", "manifests": manifests,
        "include_graph": loaded.include_graph, "leaves": leaves,
        "named_source": {"canonical": source.source_root, "evidence": source.resolution_evidence, "assets": assets, "discovery_reads": namespace},
        "source_files": files, "source_directories": dirs, "local_link_existence": existence,
        "effigy": bytes(&binary)?, "resource_authority": "unavailable-without-live-controller"
    })).map_err(refuse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualified_cli_spelling_rejects_passthrough_and_path_bypass() {
        let parsed =
            effigy_cli::parse_command(["docs", "qualified", "resolve"].map(str::to_owned)).unwrap();
        assert!(matches!(
            parsed,
            effigy_cli::Command::Docs(effigy_cli::DocsArgs {
                subcommand: effigy_cli::DocsSubcommand::QualifiedResolve,
                output_json: true,
                ..
            })
        ));
        for tail in ["--", "--repo", "--path", "--json", "extra"] {
            assert!(effigy_cli::parse_command(
                ["docs", "qualified", "resolve", tail].map(str::to_owned)
            )
            .is_err());
        }
    }

    #[test]
    fn qualified_literal_argv_refuses_shell_and_wrappers() {
        for command in [
            "effigy docs check links; echo bad",
            "env effigy docs check links && false",
            "$(helper)",
            "effigy docs check links > output",
            "effigy docs check links {args}",
        ] {
            assert!(literal_words(command).is_err(), "{command}");
        }
        assert_eq!(
            literal_words("effigy docs check links").unwrap(),
            ["effigy", "docs", "check", "links"]
        );
    }

    #[test]
    fn qualified_read_escape_is_rejected_lexically() {
        let root = Path::new("/private/fixture");
        assert!(inside(root, &root.join("../foreign/file.md")).is_err());
        assert_eq!(
            inside(root, &root.join("docs/../README.md")).unwrap(),
            root.join("README.md")
        );
    }

    #[test]
    fn qualified_selected_task_configuration_refuses_unsupported_shapes() {
        for setting in [
            "env = { KEY='value' }",
            "admission='heavy'",
            "run_in='container'",
            "mode='tui'",
            "env_file='.env'",
            "cache={enabled=true}",
            "lock='owned'",
            "profiles={alternate={}}",
            "secrets='required'",
        ] {
            let manifest: toml::Value = toml::from_str(&format!(
                "[tasks.leaf]\nrun='effigy docs check links'\n{setting}\n"
            ))
            .unwrap();
            assert!(task_run(&manifest, "leaf").is_err(), "{setting}");
        }
    }
}
