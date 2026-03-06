use super::{for_each_manifest_command, for_each_manifest_task_reference};
use crate::runner::TaskManifest;

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
          { task = "shell" },
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
