use std::collections::BTreeMap;

use super::{ManifestTask, ManifestTaskLikeDefinition};

pub fn deserialize_tasks<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, ManifestTask>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let definitions =
        <BTreeMap<String, toml::Value> as serde::Deserialize>::deserialize(deserializer)?;
    definitions
        .into_iter()
        .map(|(name, mut value)| {
            // Discovery metadata belongs to every task shorthand, including a
            // run-step table with timeout/retry fields.
            let (visibility, group) = if let Some(table) = value.as_table_mut() {
                (table.remove("visibility"), table.remove("group"))
            } else {
                (None, None)
            };
            let definition: ManifestTaskLikeDefinition =
                value.try_into().map_err(serde::de::Error::custom)?;
            let mut task = definition.into_manifest_task();
            task.visibility = visibility
                .map(toml::Value::try_into)
                .transpose()
                .map_err(serde::de::Error::custom)?;
            task.group = group
                .map(toml::Value::try_into)
                .transpose()
                .map_err(serde::de::Error::custom)?;
            Ok((name, task))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::deserialize_tasks;
    use crate::{
        ManifestManagedRun, ManifestManagedRunStep, ManifestTaskRunIn, ManifestTaskSecretsMode,
    };

    #[derive(Debug, serde::Deserialize)]
    struct TasksEnvelope {
        #[serde(deserialize_with = "deserialize_tasks")]
        tasks: std::collections::BTreeMap<String, crate::ManifestTask>,
    }

    #[test]
    fn task_visibility_metadata_survives_full_and_compact_forms() {
        let parsed: TasksEnvelope = toml::from_str(
            r#"
[tasks]
full = { run = "echo full", visibility = "internal", group = "Checks" }
compact = { task = "full", visibility = "public", group = "Development" }
step = { run = "echo step", timeout_ms = 500, visibility = "internal", group = "Checks" }
"#,
        )
        .expect("visibility keys");
        assert_eq!(
            parsed.tasks["full"].visibility,
            Some(crate::ManifestTaskVisibility::Internal)
        );
        assert_eq!(
            parsed.tasks["compact"].visibility,
            Some(crate::ManifestTaskVisibility::Public)
        );
        assert_eq!(
            parsed.tasks["compact"].group.as_deref(),
            Some("Development")
        );
        assert!(toml::from_str::<TasksEnvelope>(
            "[tasks.x]\nrun = 'echo x'\nvisibility = 'private'"
        )
        .is_err());
    }

    #[test]
    fn shorthand_task_definition_accepts_single_task_object_without_array_wrapper() {
        let parsed: TasksEnvelope = toml::from_str(
            r#"
[tasks]
sync = { task = "defer migrate/media https://www.example.test" }
"#,
        )
        .expect("parse shorthand task definition");

        let task = parsed.tasks.get("sync").expect("missing sync task");
        let Some(ManifestManagedRun::Sequence(steps)) = &task.run else {
            panic!("expected shorthand single task object to deserialize as one-step sequence");
        };
        assert!(matches!(
            steps.as_slice(),
            [ManifestManagedRunStep::Step(step)]
                if step.task.as_deref() == Some("defer migrate/media https://www.example.test")
        ));
    }

    #[test]
    fn shorthand_task_definition_accepts_task_level_run_in() {
        let parsed: TasksEnvelope = toml::from_str(
            r#"
[tasks]
capture = { rhai = "scripts/capture.rhai", run_in = "host" }
"#,
        )
        .expect("parse shorthand task definition with run_in");

        let task = parsed.tasks.get("capture").expect("missing capture task");
        assert_eq!(task.run_in, Some(ManifestTaskRunIn::Host));
        let Some(ManifestManagedRun::Sequence(steps)) = &task.run else {
            panic!("expected shorthand single task object to deserialize as one-step sequence");
        };
        assert!(matches!(
            steps.as_slice(),
            [ManifestManagedRunStep::Step(step)]
                if step.rhai.as_deref() == Some("scripts/capture.rhai")
        ));
    }

    #[test]
    fn task_table_run_accepts_single_task_object_without_array_wrapper() {
        let parsed: TasksEnvelope = toml::from_str(
            r#"
[tasks.release]
run = { task = "defer release" }
"#,
        )
        .expect("parse task table definition");

        let task = parsed.tasks.get("release").expect("missing release task");
        let Some(ManifestManagedRun::Sequence(steps)) = &task.run else {
            panic!("expected task-table single task object to deserialize as one-step sequence");
        };
        assert!(matches!(
            steps.as_slice(),
            [ManifestManagedRunStep::Step(step)] if step.task.as_deref() == Some("defer release")
        ));
    }

    #[test]
    fn task_table_accepts_secrets_required_mode() {
        let parsed: TasksEnvelope = toml::from_str(
            r#"
[tasks.dev]
mode = "tui"
container_lifecycle = true
secrets = "required"
concurrent = [
  { role = "lifecycle" },
]
"#,
        )
        .expect("parse task table definition");

        let task = parsed.tasks.get("dev").expect("missing dev task");
        assert_eq!(task.secrets, Some(ManifestTaskSecretsMode::Required));
    }

    #[test]
    fn task_table_accepts_the_declared_heavy_admission_class() {
        let parsed: TasksEnvelope = toml::from_str(
            r#"
[tasks.qa]
admission = "heavy"
run = "cargo test"
"#,
        )
        .expect("parse heavy task");

        let task = parsed.tasks.get("qa").expect("qa task");
        assert_eq!(task.admission, Some(crate::ManifestTaskAdmission::Heavy));
    }

    #[test]
    fn task_table_rejects_unknown_admission_classes() {
        let error = toml::from_str::<crate::ManifestTask>("admission = \"urgent\"")
            .expect_err("unknown admission class must fail closed");

        assert!(error.to_string().contains("urgent"));
    }

    #[test]
    fn repository_task_config_parses_heavy_task_tables() {
        let parsed: TasksEnvelope = toml::from_str(include_str!("../../../config/tasks.toml"))
            .expect("parse repository tasks config");

        for name in ["qa:ci:fast", "qa:ci:local"] {
            assert_eq!(
                parsed.tasks.get(name).and_then(|task| task.admission),
                Some(crate::ManifestTaskAdmission::Heavy),
                "{name} should be declared heavy"
            );
        }
    }
}
