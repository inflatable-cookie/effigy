use std::collections::BTreeMap;

use super::{
    ManifestInlineTaskDefinition, ManifestManagedRun, ManifestManagedRunStep,
    ManifestManagedRunStepTable, ManifestTask,
};

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum ManifestTaskDefinition {
    Run(String),
    RunSequence(Vec<ManifestManagedRunStep>),
    Full(Box<ManifestTask>),
    Compact(Box<ManifestInlineTaskDefinition>),
    RunStep(Box<ManifestManagedRunStepTable>),
}

impl ManifestTaskDefinition {
    fn into_manifest_task(self) -> ManifestTask {
        match self {
            ManifestTaskDefinition::Run(command) => ManifestTask {
                run: Some(ManifestManagedRun::Command(command)),
                ..ManifestTask::default()
            },
            ManifestTaskDefinition::RunSequence(sequence) => ManifestTask {
                run: Some(ManifestManagedRun::Sequence(sequence)),
                ..ManifestTask::default()
            },
            ManifestTaskDefinition::RunStep(step) => ManifestTask {
                run: Some(ManifestManagedRun::Sequence(vec![
                    ManifestManagedRunStep::Step(step),
                ])),
                ..ManifestTask::default()
            },
            ManifestTaskDefinition::Compact(task) => task.into_manifest_task(),
            ManifestTaskDefinition::Full(task) => *task,
        }
    }
}

pub fn deserialize_tasks<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, ManifestTask>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let definitions =
        <BTreeMap<String, ManifestTaskDefinition> as serde::Deserialize>::deserialize(
            deserializer,
        )?;
    Ok(definitions
        .into_iter()
        .map(|(name, definition)| (name, definition.into_manifest_task()))
        .collect())
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
}
