use effigy_manifest::task_runtime::{
    ManifestManagedConcurrentEntry, ManifestManagedRun, ManifestManagedRunStep, ManifestTask,
};
use effigy_manifest::TaskManifest;

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
                            let table = table.as_ref();
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
                        let table = table.as_ref();
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
            if reference.trim() == "shell" {
                continue;
            }
            visit(reference);
        }
    }
}

#[cfg(test)]
#[path = "task_graph/tests.rs"]
mod tests;
