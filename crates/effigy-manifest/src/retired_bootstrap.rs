//! `[bootstrap]` keys retired in v0.15 are translated for one release.
//!
//! Q-009 rebuilt `effigy bootstrap` around ordinary task selectors
//! (`setup`/`start`) and pinned `[sources.<name>]`. A v0.14.1 project must keep
//! loading after an update (contract 052 §11, `docs/knowledge/vision.md`), so
//! every retired `[bootstrap]` spelling is read here instead of refused:
//!
//! - `run` is read as `setup`; `{ task = ... }` entries become selector
//!   strings, and inline `run`/`rhai`/command steps become internal tasks the
//!   `setup` list selects, so they stay runnable.
//! - `{ task = ... }` entries in `setup` are read as `"..."` selectors.
//! - `submodules = "none"|"init"|"recursive"` is read as a boolean.
//! - a multi-selector `start` chain runs as one internal task.
//! - `[bootstrap].children` and the retired `bootstrap:db-seed` callback keep
//!   loading, but only `effigy bootstrap` refuses them, with the migration
//!   message this module carries.
//!
//! One concise notice per manifest names the translation; it is emitted by
//! [`crate::retired_notice`] once per checkout per manifest digest. The
//! translation is removed after v0.15.

use std::collections::BTreeSet;

use toml::Value;

use crate::retired_config::RetiredConfigDeclaration;

/// The `[bootstrap].children` migration, shared with `effigy bootstrap`.
pub const CHILDREN_REFUSAL: &str = "`[bootstrap].children` was removed in v0.15; declare each sibling as a pinned `[sources.<name>]` table with `git`, `rev` (or `rev_file`) and `path`, and let the repo's `setup` selectors prepare it (see CHANGELOG.md)";

/// The `bootstrap:db-seed` callback migration, shared with `effigy bootstrap`.
pub const DB_SEED_TASK_REFUSAL: &str = "the `bootstrap:db-seed` task was retired in v0.15; rename it to `data:seed` (the callback `effigy container data seed` runs) and select it from `[bootstrap].setup` when a fresh checkout needs a first seed (see CHANGELOG.md)";

const CHILDREN_COMMAND_REFUSAL: &str = "`effigy bootstrap children status|sync` was removed in v0.15; declare each sibling as a pinned `[sources.<name>]` table (`git`, `rev` or `rev_file`, `path`) and inspect it with `effigy doctor` (see CHANGELOG.md)";
const DEPS_SYNC_REFUSAL: &str = "`effigy bootstrap deps sync` was removed in v0.15; dependency preparation is an ordinary repo task in `[bootstrap].setup` (see CHANGELOG.md)";
const TEARDOWN_REFUSAL: &str = "`effigy bootstrap teardown` was removed in v0.15; environments own reset, stop them with `effigy container <name> down` (see CHANGELOG.md)";

const DB_SEED_TASK: &str = "bootstrap:db-seed";

/// Read every retired `[bootstrap]` key in `root` and rewrite the ones that
/// still have a faithful mapping. Retired-only surfaces are recorded for
/// `effigy bootstrap` to refuse.
pub(crate) fn translate(root: &mut Value) -> RetiredConfigDeclaration {
    let Some(root_table) = root.as_table_mut() else {
        return RetiredConfigDeclaration::default();
    };

    let taken_names = root_table
        .get("tasks")
        .and_then(Value::as_table)
        .map(|tasks| tasks.keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let mut translation = Translation::new(taken_names);

    if root_table
        .get("tasks")
        .and_then(Value::as_table)
        .is_some_and(|tasks| tasks.contains_key(DB_SEED_TASK))
    {
        translation.declaration.record(
            "the `bootstrap:db-seed` task stays an ordinary task and is no longer a bootstrap callback",
        );
        translation.declaration.refuse(DB_SEED_TASK_REFUSAL);
    }

    let Some(bootstrap) = root_table.remove("bootstrap") else {
        return translation.declaration;
    };
    let Value::Table(mut bootstrap) = bootstrap else {
        // Not a table: leave the type error to the manifest deserializer.
        root_table.insert("bootstrap".to_owned(), bootstrap);
        return translation.declaration;
    };

    let mut wrote_setup = false;

    if let Some(run) = bootstrap.remove("run") {
        wrote_setup = true;
        translation
            .declaration
            .record("`[bootstrap].run` is read as `setup`");
        for (index, entry) in entries(run).into_iter().enumerate() {
            translate_entry(
                entry,
                format!("bootstrap:run:{}", index + 1),
                false,
                &mut translation,
            );
        }
    }

    if let Some(value) = bootstrap.remove("setup") {
        wrote_setup = true;
        // v0.14.1 accepted `setup` as an alias for `run`, so a non-array value
        // is the old inline definition rather than a selector list.
        match value {
            Value::Array(_) => {
                for (index, entry) in entries(value).into_iter().enumerate() {
                    translate_entry(
                        entry,
                        format!("bootstrap:setup:{}", index + 1),
                        true,
                        &mut translation,
                    );
                }
            }
            other => {
                translation
                    .declaration
                    .record("an inline `[bootstrap].setup` definition runs as an internal task");
                synthesise_task(other, "bootstrap:setup".to_owned(), &mut translation);
            }
        }
    }

    if wrote_setup {
        bootstrap.insert("setup".to_owned(), Value::Array(translation.setup.clone()));
    }

    if let Some(start) = bootstrap.remove("start") {
        if matches!(start, Value::String(_) | Value::Array(_) | Value::Table(_)) {
            // A start chain writes no `setup` selector of its own.
            let translated = translate_start(start, &mut translation);
            // A start chain becomes one internal task, so only a single
            // selector is written back here.
            if let Some(selector) = translated {
                bootstrap.insert("start".to_owned(), Value::String(selector));
            }
        } else {
            // Not a retired shape; leave the type error to the deserializer.
            bootstrap.insert("start".to_owned(), start);
        }
    }

    if let Some(Value::String(policy)) = bootstrap.get("submodules") {
        let policy = policy.clone();
        let recursive = !policy.trim().eq_ignore_ascii_case("none");
        bootstrap.insert("submodules".to_owned(), Value::Boolean(recursive));
        translation.declaration.record(format!(
            "`[bootstrap].submodules = \"{policy}\"` is read as `submodules = {recursive}`"
        ));
    }

    if bootstrap.remove("children").is_some() {
        translation
            .declaration
            .record("`[bootstrap].children` is retired in favour of pinned `[sources.<name>]`");
        translation.declaration.refuse(CHILDREN_REFUSAL);
    }

    root_table.insert("bootstrap".to_owned(), Value::Table(bootstrap));

    if !translation.synthesised.is_empty() {
        let tasks = root_table
            .entry("tasks".to_owned())
            .or_insert_with(|| Value::Table(toml::map::Map::new()));
        if let Value::Table(tasks) = tasks {
            for (name, task) in translation.synthesised {
                tasks.insert(name, task);
            }
        }
    }

    translation.declaration
}

/// Everything one translation pass writes: the `setup` selectors, the internal
/// tasks it synthesises, the task names already taken, and the notices/refusals
/// it records. One value owns all four, so no entry translation builds a
/// four-field borrow bundle of its own.
struct Translation {
    setup: Vec<Value>,
    synthesised: Vec<(String, Value)>,
    taken_names: BTreeSet<String>,
    declaration: RetiredConfigDeclaration,
}

impl Translation {
    fn new(taken_names: BTreeSet<String>) -> Self {
        Self {
            setup: Vec::new(),
            synthesised: Vec::new(),
            taken_names,
            declaration: RetiredConfigDeclaration::default(),
        }
    }
}

/// A retired definition holds either one entry or an ordered list of them.
fn entries(value: Value) -> Vec<Value> {
    match value {
        Value::Array(entries) => entries,
        other => vec![other],
    }
}

/// Translate one `run`/`setup` entry into a `setup` selector.
///
/// `bare_strings_are_selectors` distinguishes the new `setup` shape (a selector
/// list) from a v0.14.1 `run` list, where a bare string is a command step.
fn translate_entry(
    entry: Value,
    synthesised_name: String,
    bare_strings_are_selectors: bool,
    output: &mut Translation,
) {
    match entry {
        Value::String(text) if bare_strings_are_selectors => {
            push_selector(text, output);
        }
        Value::String(command) => {
            output
                .declaration
                .record("inline `[bootstrap].run` steps run as internal tasks");
            let mut task = toml::map::Map::new();
            task.insert(
                "visibility".to_owned(),
                Value::String("internal".to_owned()),
            );
            task.insert("run".to_owned(), Value::String(command));
            insert_synthesised(synthesised_name, Value::Table(task), output);
        }
        Value::Table(table) => {
            let only_task = table.len() == 1 && table.contains_key("task");
            if only_task {
                if let Some(task) = table.get("task").and_then(Value::as_str) {
                    output
                        .declaration
                        .record("`{ task = ... }` bootstrap entries are read as selectors");
                    push_selector(task.to_owned(), output);
                    return;
                }
            }
            synthesise_task(Value::Table(table), synthesised_name, output);
        }
        other => synthesise_task(other, synthesised_name, output),
    }
}

/// Build an internal task from one retired inline definition.
fn synthesise_task(value: Value, name: String, output: &mut Translation) {
    let mut task = toml::map::Map::new();
    task.insert(
        "visibility".to_owned(),
        Value::String("internal".to_owned()),
    );
    match value {
        Value::Table(mut table) => {
            if let Some(run_in) = table.remove("run_in") {
                task.insert("run_in".to_owned(), run_in);
            }
            match table.remove("task") {
                Some(task_reference) => {
                    let mut step = toml::map::Map::new();
                    step.insert("task".to_owned(), task_reference);
                    for (key, value) in table {
                        step.insert(key, value);
                    }
                    task.insert("run".to_owned(), Value::Array(vec![Value::Table(step)]));
                }
                None if is_step_table(&table) => {
                    task.insert("run".to_owned(), Value::Array(vec![Value::Table(table)]));
                }
                None => {
                    for (key, value) in table {
                        task.insert(key, value);
                    }
                }
            }
        }
        Value::Array(sequence) => {
            task.insert("run".to_owned(), Value::Array(sequence));
        }
        other => {
            task.insert("run".to_owned(), Value::Array(vec![other]));
        }
    }
    let name = insert_synthesised(name, Value::Table(task), output);
    push_selector(name, output);
}

/// Keys an ordinary `[tasks.<name>.run]` step table accepts.
fn is_step_table(table: &toml::map::Map<String, Value>) -> bool {
    const STEP_KEYS: &[&str] = &["run", "task", "rhai", "env", "env_file", "id", "timeout_ms"];
    table.keys().all(|key| STEP_KEYS.contains(&key.as_str()))
}

fn insert_synthesised(name: String, task: Value, output: &mut Translation) -> String {
    let name = unique_task_name(name, &mut output.taken_names);
    output.synthesised.push((name.clone(), task));
    name
}

fn unique_task_name(mut name: String, taken: &mut BTreeSet<String>) -> String {
    let base = name.clone();
    let mut suffix = 2;
    while taken.contains(&name) {
        name = format!("{base}:{suffix}");
        suffix += 1;
    }
    taken.insert(name.clone());
    name
}

/// Record one selector and the retired bootstrap subcommands it still names.
fn push_selector(selector: String, output: &mut Translation) {
    if let Some(refusal) = retired_bootstrap_subcommand(&selector) {
        output.declaration.refuse(refusal);
    }
    output.setup.push(Value::String(selector));
}

/// `bootstrap children …`, `bootstrap deps sync …` and `bootstrap teardown`
/// were bootstrap subcommands, not task names, in v0.14.1.
fn retired_bootstrap_subcommand(selector: &str) -> Option<&'static str> {
    let mut words = selector.split_whitespace();
    if words.next() != Some("bootstrap") {
        return None;
    }
    match words.next() {
        Some("children") => Some(CHILDREN_COMMAND_REFUSAL),
        Some("deps") => Some(DEPS_SYNC_REFUSAL),
        Some("teardown") => Some(TEARDOWN_REFUSAL),
        _ => None,
    }
}

/// Translate a retired `start` value into one selector, or into one internal
/// task that sequences the chain.
fn translate_start(value: Value, output: &mut Translation) -> Option<String> {
    match value {
        Value::String(selector) => Some(selector),
        Value::Table(table) => {
            if table.len() == 1 {
                if let Some(task) = table.get("task").and_then(Value::as_str) {
                    output
                        .declaration
                        .record("a `{ task = ... }` `[bootstrap].start` is read as a selector");
                    return Some(task.to_owned());
                }
            }
            output
                .declaration
                .record("an inline `[bootstrap].start` definition runs as an internal task");
            synthesise_start_task(vec![Value::Table(table)], output);
            Some(last_synthesised_name(output))
        }
        Value::Array(entries) => {
            let selectors = entries
                .iter()
                .map(start_entry_selector)
                .collect::<Option<Vec<_>>>();
            match selectors {
                Some(selectors) if selectors.len() == 1 => selectors.into_iter().next(),
                Some(selectors) => {
                    output.declaration.record(
                        "a multi-selector `[bootstrap].start` chain runs as one internal task",
                    );
                    let steps = selectors
                        .into_iter()
                        .map(|selector| {
                            let mut step = toml::map::Map::new();
                            step.insert("task".to_owned(), Value::String(selector));
                            Value::Table(step)
                        })
                        .collect();
                    synthesise_start_task(steps, output);
                    Some(last_synthesised_name(output))
                }
                None => {
                    output
                        .declaration
                        .record("an inline `[bootstrap].start` chain runs as one internal task");
                    synthesise_start_task(entries, output);
                    Some(last_synthesised_name(output))
                }
            }
        }
        _ => None,
    }
}

fn start_entry_selector(entry: &Value) -> Option<String> {
    match entry {
        Value::String(selector) => Some(selector.clone()),
        Value::Table(table) if table.len() == 1 => {
            table.get("task").and_then(Value::as_str).map(str::to_owned)
        }
        _ => None,
    }
}

fn synthesise_start_task(steps: Vec<Value>, output: &mut Translation) {
    let mut task = toml::map::Map::new();
    task.insert(
        "visibility".to_owned(),
        Value::String("internal".to_owned()),
    );
    task.insert("run".to_owned(), Value::Array(steps));
    insert_synthesised("bootstrap:start".to_owned(), Value::Table(task), output);
}

fn last_synthesised_name(output: &Translation) -> String {
    output
        .synthesised
        .last()
        .map(|(name, _)| name.clone())
        .expect("a start chain always synthesises a task")
}

#[cfg(test)]
mod tests {
    use super::DB_SEED_TASK_REFUSAL;
    use crate::TaskManifest;

    fn parse(text: &str) -> TaskManifest {
        toml::from_str(text).expect("retired bootstrap keys keep loading")
    }

    fn setup(manifest: &TaskManifest) -> Vec<String> {
        manifest
            .bootstrap
            .as_ref()
            .map(|bootstrap| bootstrap.setup.clone())
            .unwrap_or_default()
    }

    #[test]
    fn a_multi_selector_start_chain_runs_as_one_internal_task() {
        let manifest = parse(
            r#"
[bootstrap]
start = [{ task = "container:up" }, { task = "dev" }]
"#,
        );
        assert_eq!(
            manifest
                .bootstrap
                .as_ref()
                .and_then(|bootstrap| bootstrap.start.as_deref()),
            Some("bootstrap:start")
        );
        assert!(manifest.tasks.contains_key("bootstrap:start"));
    }

    #[test]
    fn a_submodule_policy_string_is_read_as_a_boolean() {
        let manifest = parse("[bootstrap]\nsubmodules = \"recursive\"\n");
        assert!(manifest.bootstrap.expect("bootstrap").submodules);
        let none = parse("[bootstrap]\nsubmodules = \"none\"\n");
        assert!(!none.bootstrap.expect("bootstrap").submodules);
    }

    #[test]
    fn the_db_seed_callback_task_loads_and_refuses_only_in_bootstrap() {
        let manifest = parse(
            r#"
[tasks."bootstrap:db-seed"]
run = "psql < seed.sql"
"#,
        );
        assert!(manifest.tasks.contains_key("bootstrap:db-seed"));
        assert_eq!(manifest.retired_config.refusals(), [DB_SEED_TASK_REFUSAL]);
    }

    #[test]
    fn synthesised_task_names_never_shadow_a_declared_task() {
        let manifest = parse(
            r#"
[bootstrap]
run = [{ rhai = "scripts/bootstrap.rhai" }]

[tasks."bootstrap:run:1"]
run = "echo declared"
"#,
        );
        assert_eq!(setup(&manifest), vec!["bootstrap:run:1:2"]);
        assert_eq!(
            manifest
                .tasks
                .get("bootstrap:run:1")
                .and_then(|task| match task.run.as_ref() {
                    Some(crate::ManifestManagedRun::Command(command)) => Some(command.as_str()),
                    _ => None,
                }),
            Some("echo declared")
        );
    }

    #[test]
    fn a_plain_manifest_records_no_retired_bootstrap_declaration() {
        let manifest =
            parse("[bootstrap]\nsetup = [\"prepare\"]\n\n[tasks.prepare]\nrun = \"true\"\n");
        assert!(manifest.retired_config.is_empty());
        assert_eq!(setup(&manifest), vec!["prepare"]);
    }
}
