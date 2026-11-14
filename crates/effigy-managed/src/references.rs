use std::path::{Path, PathBuf};

#[path = "references/context.rs"]
mod context;
#[path = "references/parser.rs"]
mod parser;
#[path = "references/resolve.rs"]
mod resolve;

use effigy_core::shell::shell_quote;
use effigy_manifest::TaskResolverFn;

use super::run_spec::wrap_reference_command_in_cwd;
use crate::ManagedError;
use context::{ManagedRefContext, StepRefContext};
use effigy_manifest::{LoadedCatalog, TaskSelection};
pub(crate) use parser::{merge_args_rendered, parse_task_ref};
use resolve::{resolve_reference_run, ResolvedReferenceRun};

#[derive(Clone, Copy)]
pub(crate) struct ReferenceResolution<'request, 'catalog> {
    pub(crate) args_rendered: &'request str,
    pub(crate) catalogs: &'catalog [LoadedCatalog],
    pub(crate) task_scope_cwd: &'request Path,
    pub(crate) execution_root: &'request Path,
    pub(crate) invocation_cwd: &'request Path,
    pub(crate) runtime_env_schema_override: Option<&'request Path>,
    pub(crate) depth: usize,
    pub(crate) resolver: TaskResolverFn<'catalog>,
    /// Declared backing-service aliases the host execution environment
    /// resolves to this checkout's effective aliases (effigy#431).
    pub(crate) host_env_aliases: &'catalog [crate::run_spec::HostAlias],
    /// True when the rendered command will run on the host (suite/sequence
    /// expansion). False when a managed parent will already exec it in a
    /// selected container.
    pub(crate) host_launched: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_task_reference_run<'a>(
    managed_task_name: &str,
    process_name: &str,
    task_ref: &str,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
    host_env_aliases: &'a [crate::run_spec::HostAlias],
    resolver: TaskResolverFn<'a>,
) -> Result<(crate::run_spec::RenderedRun, PathBuf), ManagedError> {
    let context = ManagedRefContext {
        managed_task_name: managed_task_name.to_owned(),
        process_name: process_name.to_owned(),
        task_ref: task_ref.to_owned(),
    };
    let resolved = resolve_task_reference(
        task_ref,
        ReferenceResolution {
            args_rendered: "",
            catalogs,
            task_scope_cwd,
            execution_root: task_scope_cwd,
            invocation_cwd: task_scope_cwd,
            runtime_env_schema_override: None,
            depth: 0,
            resolver,
            host_env_aliases,
            host_launched: false,
        },
        |detail| context.invalid(detail),
        |detail| context.invalid(detail),
        |parsed, selection| {
            context.invalid(format!(
                "referenced task `{}` in {} has no `run` command",
                parsed.selector.task_name,
                selection.catalog.manifest_path.display()
            ))
        },
    )?;
    Ok((resolved.run, resolved.cwd))
}

pub(crate) fn resolve_task_reference_step<'request, 'catalog>(
    task_name: &str,
    task_ref: &str,
    resolution: ReferenceResolution<'request, 'catalog>,
) -> Result<String, ManagedError> {
    let context = StepRefContext {
        task_name: task_name.to_owned(),
        task_ref: task_ref.to_owned(),
    };
    let resolved = resolve_task_reference(
        task_ref,
        resolution,
        |detail| context.invalid(detail),
        |detail| context.failure(detail),
        |_, selection| {
            context.failure(format!(
                "task `{task_name}` run step task ref `{task_ref}` has no `run` command in {}",
                selection.catalog.manifest_path.display()
            ))
        },
    )?;
    Ok(wrap_reference_command_in_cwd(
        &resolved.cwd,
        &resolved.run.command,
    ))
}

fn resolve_task_reference<'a, FInvalid, FResolve, FMissing>(
    task_ref: &str,
    resolution: ReferenceResolution<'_, 'a>,
    invalid_error: FInvalid,
    resolve_error: FResolve,
    missing_run_error: FMissing,
) -> Result<ResolvedReferenceRun, ManagedError>
where
    FInvalid: Fn(String) -> ManagedError,
    FResolve: Fn(String) -> ManagedError,
    FMissing: Fn(&parser::ParsedTaskRef, &TaskSelection<'a>) -> ManagedError,
{
    let parsed = parse_task_ref(&expand_reference_placeholders(
        task_ref,
        resolution.execution_root,
        resolution.task_scope_cwd,
    ))
    .map_err(|error| invalid_error(error.to_string()))?;
    let merged_args_rendered = merge_args_rendered(&parsed.args_rendered, resolution.args_rendered);
    resolve_reference_run(
        &parsed,
        ReferenceResolution {
            args_rendered: &merged_args_rendered,
            ..resolution
        },
        |selection| missing_run_error(&parsed, selection),
    )
    .map_err(|error| resolve_error(error.to_string()))
}

/// Expand the path placeholders a manifest run command already supports inside
/// a `{ task = "…" }` reference.
///
/// `{repo}` is the task's execution root (the selected catalog root) and
/// `{cwd}` is the invoking task's cwd, so a reference has a spelling that works
/// from a nested catalog (`{ task = "docs check links {repo}/README.md" }`).
/// Paths are shell-quoted because the reference's arguments are parsed as
/// shell words.
pub fn expand_reference_placeholders(
    task_ref: &str,
    execution_root: &Path,
    task_scope_cwd: &Path,
) -> String {
    let repo_rendered = shell_quote(&execution_root.display().to_string());
    let cwd_rendered = shell_quote(&task_scope_cwd.display().to_string());
    task_ref
        .replace("{repo}", &repo_rendered)
        .replace("{cwd}", &cwd_rendered)
}

#[cfg(test)]
mod tests {
    use super::context::{ManagedRefContext, StepRefContext};
    use super::parser::{is_builtin_task_selector, merge_args_rendered, parse_task_ref};
    use super::{expand_reference_placeholders, resolve_task_reference_run};
    use effigy_manifest::{load_task_manifest, LoadedCatalog, TaskResolveError, TaskSelection};
    use effigy_tasks::CatalogSelectionMode;
    use std::path::Path;

    fn catalog(root: &Path, manifest: &str) -> LoadedCatalog {
        std::fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
        let manifest = load_task_manifest(&root.join("effigy.toml")).expect("manifest");
        LoadedCatalog {
            alias: "root".to_owned(),
            catalog_root: root.to_path_buf(),
            manifest_path: root.join("effigy.toml"),
            bundle_root: None,
            manifest,
            effective_digest: None,
            defer_run: None,
            deferred_builtins: Default::default(),
            depth: 0,
        }
    }

    fn select<'c>(
        selector: &effigy_tasks::TaskSelector,
        catalogs: &'c [LoadedCatalog],
    ) -> Result<TaskSelection<'c>, TaskResolveError> {
        let catalog = catalogs
            .first()
            .ok_or_else(|| TaskResolveError::not_found("no catalog"))?;
        let task = catalog
            .manifest
            .tasks
            .get(&selector.task_name)
            .ok_or_else(|| {
                TaskResolveError::not_found(format!("unknown task `{}`", selector.task_name))
            })?;
        Ok(TaskSelection {
            catalog,
            task,
            mode: CatalogSelectionMode::CwdNearest,
            evidence: vec!["fixture".to_owned()],
            surface: effigy_core::task_selection::TaskSurface::Published,
        })
    }

    #[test]
    fn a_reference_keeps_its_selector_and_shell_quoted_arguments() {
        let parsed = parse_task_ref("docs check links 'two words.rs'").expect("parse reference");
        assert_eq!(parsed.selector.task_name, "docs");
        assert_eq!(parsed.selector_rendered, "docs");
        assert_eq!(parsed.args_rendered, "'check' 'links' 'two words.rs'");
        assert_eq!(
            merge_args_rendered(&parsed.args_rendered, "'extra' 'a b'"),
            "'check' 'links' 'two words.rs' 'extra' 'a b'"
        );
        assert_eq!(merge_args_rendered("", ""), "");
        assert_eq!(merge_args_rendered("left", ""), "left");
        assert_eq!(merge_args_rendered("", "right"), "right");
    }

    #[test]
    fn an_invalid_reference_is_named_by_the_step_that_declared_it() {
        let error = parse_task_ref("docs 'unterminated").expect_err("unbalanced quotes");
        let step = StepRefContext {
            task_name: "release".to_owned(),
            task_ref: "{ task = \"docs 'unterminated\" }".to_owned(),
        };
        let named = step.invalid(error.to_string());
        let text = named.to_string();
        assert!(text.contains("task `release` run step task ref"), "{text}");
        assert!(text.contains("is invalid"), "{text}");

        let managed = ManagedRefContext {
            managed_task_name: "dev".to_owned(),
            process_name: "serve".to_owned(),
            task_ref: "docs 'unterminated".to_owned(),
        };
        let invalid = managed.invalid("bad quotes").to_string();
        assert!(invalid.contains("dev"), "{invalid}");
        assert!(invalid.contains("serve"), "{invalid}");
        assert!(invalid.contains("bad quotes"), "{invalid}");
    }

    #[test]
    fn placeholders_are_shell_quoted_so_a_path_with_spaces_stays_one_argument() {
        let expanded = expand_reference_placeholders(
            "{ task = \"docs check {repo}/README.md {cwd}\" }",
            Path::new("/tmp/repo with spaces"),
            Path::new("/tmp/work"),
        );
        assert!(
            expanded.contains("'/tmp/repo with spaces'/README.md"),
            "{expanded}"
        );
        assert!(expanded.contains("'/tmp/work'"), "{expanded}");
    }

    #[test]
    fn a_reference_resolves_through_the_repository_resolver_not_a_second_lookup() {
        let home = tempfile::tempdir().expect("fixture");
        let catalogs = [catalog(
            home.path(),
            r#"
[tasks.build]
run = "echo building"
"#,
        )];
        let (rendered, cwd) = resolve_task_reference_run(
            "release",
            "publish",
            "build --release",
            &catalogs,
            home.path(),
            &[],
            &|selector, catalogs, _| select(selector, catalogs),
        )
        .expect("resolve reference");
        assert!(rendered.command.contains("echo building"), "{rendered:?}");
        assert_eq!(cwd, home.path());
    }

    #[test]
    fn a_task_without_a_run_command_names_the_reference_that_named_it() {
        let home = tempfile::tempdir().expect("fixture");
        let catalogs = [catalog(
            home.path(),
            r#"
[tasks.empty]
description = "no run declared"
"#,
        )];
        let error = resolve_task_reference_run(
            "release",
            "publish",
            "empty",
            &catalogs,
            home.path(),
            &[],
            &|selector, catalogs, _| select(selector, catalogs),
        )
        .expect_err("a task without `run` cannot be referenced");
        let text = error.to_string();
        assert!(text.contains("has no `run` command"), "{text}");
        assert!(text.contains("empty"), "{text}");
    }

    #[test]
    fn an_unknown_reference_keeps_the_resolvers_own_refusal() {
        let home = tempfile::tempdir().expect("fixture");
        let catalogs = [catalog(home.path(), "[tasks.build]\nrun = \"true\"\n")];
        let error = resolve_task_reference_run(
            "release",
            "publish",
            "missing",
            &catalogs,
            home.path(),
            &[],
            &|selector, catalogs, _| select(selector, catalogs),
        )
        .expect_err("unknown reference");
        assert!(
            error.to_string().contains("unknown task `missing`"),
            "{error}"
        );

        let selector = parse_task_ref("changelog")
            .expect("builtin reference")
            .selector;
        assert!(is_builtin_task_selector(&selector));
    }
}
