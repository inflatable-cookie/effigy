//! One-release, in-memory translation into the single task namespace.
use serde::Deserialize;
use toml::Value;

use crate::TaskManifest;

const WARNING: &str = "[warn] `[drafts]` and draft references are deprecated in v0.15; move definitions to `[tasks.<name>]` with `visibility = \"internal\"`, remove `created`/`expires`/`purpose`, and replace `draft` references with `task`. Translation is supported for one release.";

impl<'de> Deserialize<'de> for TaskManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = Value::deserialize(deserializer)?;
        let migrated = migrate(&mut value).map_err(serde::de::Error::custom)?;
        let manifest = TaskManifest::deserialize(value).map_err(serde::de::Error::custom)?;
        if migrated {
            eprintln!("{WARNING}");
        }
        Ok(manifest)
    }
}

fn migrate(value: &mut Value) -> Result<bool, String> {
    let Some(root) = value.as_table_mut() else {
        return Ok(false);
    };
    let mut migrated = false;
    if let Some(drafts) = root.remove("drafts") {
        let Value::Table(drafts) = drafts else {
            return Err("`[drafts]` must be a table; migrate to internal `[tasks]`".to_owned());
        };
        let tasks = root
            .entry("tasks")
            .or_insert_with(|| Value::Table(Default::default()));
        let tasks = tasks.as_table_mut().ok_or("`[tasks]` must be a table")?;
        for (name, draft) in drafts {
            if tasks.contains_key(&name) {
                return Err(format!("`{name}` is declared in both `[tasks]` and `[drafts]`; rename one before migrating to the single task namespace"));
            }
            let Value::Table(mut body) = draft else {
                return Err(format!("legacy draft `{name}` must be a full table; move it to `[tasks.{name}]` with `visibility = \"internal\"`"));
            };
            for key in ["created", "expires", "purpose"] {
                body.remove(key);
            }
            body.insert(
                "visibility".to_owned(),
                Value::String("internal".to_owned()),
            );
            tasks.insert(name, Value::Table(body));
        }
        migrated = true;
    }
    if let Some(tasks) = root.get_mut("tasks").and_then(Value::as_table_mut) {
        for (_, task) in tasks.iter_mut() {
            migrated |= migrate_task(task)?;
        }
    }
    // Bounded QA group members also share the one task namespace.
    if let Some(groups) = root
        .get_mut("qa")
        .and_then(|qa| qa.get_mut("groups"))
        .and_then(Value::as_table_mut)
    {
        for (_, group) in groups.iter_mut() {
            if let Some(members) = group.get_mut("members").and_then(Value::as_array_mut) {
                for member in members {
                    if member.get("surface").and_then(Value::as_str) == Some("draft") {
                        member["surface"] = Value::String("published".to_owned());
                        migrated = true;
                    }
                }
            }
        }
    }
    Ok(migrated)
}

fn migrate_task(value: &mut Value) -> Result<bool, String> {
    let mut migrated = false;
    match value {
        Value::Array(steps) => {
            for step in steps {
                migrated |= migrate_step(step)?;
            }
        }
        Value::Table(body) => {
            if let Some(run) = body.get_mut("run") {
                match run {
                    Value::Array(steps) => {
                        for step in steps {
                            migrated |= migrate_step(step)?;
                        }
                    }
                    Value::Table(_) => migrated |= migrate_step(run)?,
                    _ => {}
                }
            }
            if let Some(concurrent) = body.get_mut("concurrent").and_then(Value::as_array_mut) {
                for entry in concurrent {
                    if let Some(setup) = entry.get_mut("setup").and_then(Value::as_array_mut) {
                        for step in setup {
                            migrated |= migrate_step(step)?;
                        }
                    }
                }
            }
            if let Some(profiles) = body.get_mut("profiles").and_then(Value::as_table_mut) {
                for (_, profile) in profiles.iter_mut() {
                    migrated |= migrate_task(profile)?;
                }
            }
        }
        _ => {}
    }
    Ok(migrated)
}

fn migrate_step(value: &mut Value) -> Result<bool, String> {
    let Some(step) = value.as_table_mut() else {
        return Ok(false);
    };
    let Some(reference) = step.remove("draft") else {
        return Ok(false);
    };
    if step.contains_key("task") {
        return Err(
            "a step cannot declare both `task` and legacy `draft`; use one `task` reference"
                .to_owned(),
        );
    }
    step.insert("task".to_owned(), reference);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use crate::{ManifestManagedRun, ManifestManagedRunStep, ManifestTaskVisibility, TaskManifest};

    #[test]
    fn drafts_migration_produces_only_internal_tasks_and_normal_references() {
        let manifest: TaskManifest = toml::from_str(
            r#"
[tasks]
entry = [{ draft = "proof" }]
[drafts.proof]
created = "2026-09-15"
expires = "2026-09-29"
purpose = "Temporary proof"
run = [{ draft = "helper" }]
[drafts.helper]
created = "2026-09-15"
purpose = "Helper"
run = "echo helper"
"#,
        )
        .expect("migration");
        assert_eq!(manifest.tasks.len(), 3);
        assert_eq!(
            manifest.tasks["proof"].visibility,
            Some(ManifestTaskVisibility::Internal)
        );
        let Some(ManifestManagedRun::Sequence(steps)) = &manifest.tasks["proof"].run else {
            panic!("sequence");
        };
        assert!(
            matches!(&steps[0], ManifestManagedRunStep::Step(step) if step.task.as_deref() == Some("helper"))
        );
    }

    #[test]
    fn drafts_migration_refuses_namespace_collisions() {
        let error = toml::from_str::<TaskManifest>(
            "[tasks]\nx = 'echo task'\n[drafts.x]\nrun = 'echo draft'",
        )
        .expect_err("collision");
        assert!(error.to_string().contains("both `[tasks]` and `[drafts]`"));
    }
}
