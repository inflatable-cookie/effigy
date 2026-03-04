use super::super::{
    ManifestManagedConcurrentEntry, ManifestManagedRun, ManifestManagedRunStep, ManifestTask,
    TaskManifest,
};

pub(super) fn for_each_manifest_command<F>(manifest: &TaskManifest, mut visit: F)
where
    F: FnMut(&str),
{
    for task in manifest.tasks.values() {
        for_each_task_command(task, &mut visit);
    }
}

pub(super) fn for_each_manifest_task_reference<F>(manifest: &TaskManifest, mut visit: F)
where
    F: FnMut(&str, &str),
{
    for (task_name, task) in &manifest.tasks {
        for_each_task_reference(task, |reference| visit(task_name, reference));
    }
}

fn for_each_task_command<F>(task: &ManifestTask, visit: &mut F)
where
    F: FnMut(&str),
{
    if let Some(run) = task.run.as_ref() {
        match run {
            ManifestManagedRun::Command(command) => visit(command),
            ManifestManagedRun::Sequence(steps) => {
                for step in steps {
                    match step {
                        ManifestManagedRunStep::Command(command) => visit(command),
                        ManifestManagedRunStep::Step(table) => {
                            if let Some(run) = table.run.as_ref() {
                                visit(run);
                            }
                        }
                    }
                }
            }
        }
    }

    for_each_concurrent_command(&task.concurrent, visit);
    for profile in task.profiles.values() {
        for_each_concurrent_command(&profile.concurrent, visit);
    }
}

fn for_each_task_reference<F>(task: &ManifestTask, mut visit: F)
where
    F: FnMut(&str),
{
    if let Some(run) = task.run.as_ref() {
        match run {
            ManifestManagedRun::Command(_) => {}
            ManifestManagedRun::Sequence(steps) => {
                for step in steps {
                    if let ManifestManagedRunStep::Step(table) = step {
                        if let Some(reference) = table.task.as_deref() {
                            visit(reference);
                        }
                    }
                }
            }
        }
    }
    for_each_concurrent_task_reference(&task.concurrent, &mut visit);
    for profile in task.profiles.values() {
        for_each_concurrent_task_reference(&profile.concurrent, &mut visit);
    }
}

fn for_each_concurrent_command<F>(entries: &[ManifestManagedConcurrentEntry], visit: &mut F)
where
    F: FnMut(&str),
{
    for entry in entries {
        if let Some(run) = entry.run.as_ref() {
            visit(run);
        }
    }
}

fn for_each_concurrent_task_reference<F>(entries: &[ManifestManagedConcurrentEntry], visit: &mut F)
where
    F: FnMut(&str),
{
    for entry in entries {
        if let Some(reference) = entry.task.as_deref() {
            visit(reference);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_manifest() -> TaskManifest {
        toml::from_str::<TaskManifest>(
            r#"
            [tasks.build]
            run = ["cargo build", { run = "node scripts/build.js", task = "ops/prebuild" }, "cargo test"]
            concurrent = [
              { run = "pnpm dev" },
              { task = "lint" }
            ]

            [tasks.build.profiles.ci]
            concurrent = [
              { run = "npm run ci" },
              { task = "ops/verify" }
            ]

            [tasks.health]
            run = "node scripts/health.js"
            concurrent = [
              { task = "ops/health-extra" }
            ]
            "#,
        )
        .expect("task graph fixture should parse")
    }

    #[test]
    fn manifest_command_walker_includes_run_sequence_and_concurrent_commands() {
        let manifest = fixture_manifest();
        let mut commands = Vec::<String>::new();

        for_each_manifest_command(&manifest, |command| commands.push(command.to_owned()));

        assert_eq!(
            commands,
            vec![
                "cargo build",
                "node scripts/build.js",
                "cargo test",
                "pnpm dev",
                "npm run ci",
                "node scripts/health.js",
            ]
        );
    }

    #[test]
    fn manifest_reference_walker_includes_sequence_and_concurrent_task_references() {
        let manifest = fixture_manifest();
        let mut references = Vec::<(String, String)>::new();

        for_each_manifest_task_reference(&manifest, |task_name, reference| {
            references.push((task_name.to_owned(), reference.to_owned()));
        });

        assert_eq!(
            references,
            vec![
                ("build".to_owned(), "ops/prebuild".to_owned()),
                ("build".to_owned(), "lint".to_owned()),
                ("build".to_owned(), "ops/verify".to_owned()),
                ("health".to_owned(), "ops/health-extra".to_owned()),
            ]
        );
    }
}
