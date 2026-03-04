use super::prelude::*;

#[test]
fn run_manifest_task_run_array_supports_task_env_file_for_dotenv_fallback() {
    let root = temp_workspace("run-array-env-value-task-env-file");
    let marker = root.join("env-value-task-env-file.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks.api]
env_file = ".env.test"
run = [
  {{ env = "DATABASE_URL" }},
  {{ run = "sh -lc 'printf %s \"$DATABASE_URL\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );
    fs::write(root.join(".env"), "DATABASE_URL=postgres://from-default\n").expect("write .env");
    fs::write(
        root.join(".env.test"),
        "DATABASE_URL=postgres://from-test\n",
    )
    .expect("write .env.test");

    assert_task_output_equals(&root, "api", &marker, "postgres://from-test");
}

#[test]
fn run_manifest_task_run_array_supports_env_file_directive_for_dotenv_fallback() {
    let root = temp_workspace("run-array-env-value-step-env-file");
    let marker = root.join("env-value-step-env-file.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks]
api = [
  {{ env_file = ".env.test" }},
  {{ env = "DATABASE_URL" }},
  {{ run = "sh -lc 'printf %s \"$DATABASE_URL\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );
    fs::write(root.join(".env"), "DATABASE_URL=postgres://from-default\n").expect("write .env");
    fs::write(
        root.join(".env.test"),
        "DATABASE_URL=postgres://from-test\n",
    )
    .expect("write .env.test");

    assert_task_output_equals(&root, "api", &marker, "postgres://from-test");
}

#[test]
fn run_manifest_task_run_array_env_file_directive_overrides_task_env_file() {
    let root = temp_workspace("run-array-env-value-step-env-file-overrides-task");
    let marker = root.join("env-value-step-env-file-overrides-task.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks.api]
env_file = ".env.test"
run = [
  {{ env_file = ".env.local" }},
  {{ env = "DATABASE_URL" }},
  {{ run = "sh -lc 'printf %s \"$DATABASE_URL\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );
    fs::write(
        root.join(".env.test"),
        "DATABASE_URL=postgres://from-test\n",
    )
    .expect("write .env.test");
    fs::write(
        root.join(".env.local"),
        "DATABASE_URL=postgres://from-local\n",
    )
    .expect("write .env.local");

    assert_task_output_equals(&root, "api", &marker, "postgres://from-local");
}

#[test]
fn run_manifest_task_run_array_supports_task_env_file_array_precedence() {
    let root = temp_workspace("run-array-env-value-task-env-file-array");
    let marker = root.join("env-value-task-env-file-array.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks.api]
env_file = [".env.local", ".env.test"]
run = [
  {{ env = "DATABASE_URL" }},
  {{ run = "sh -lc 'printf %s \"$DATABASE_URL\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );
    fs::write(
        root.join(".env.test"),
        "DATABASE_URL=postgres://from-test\n",
    )
    .expect("write .env.test");
    fs::write(
        root.join(".env.local"),
        "DATABASE_URL=postgres://from-local\n",
    )
    .expect("write .env.local");

    assert_task_output_equals(&root, "api", &marker, "postgres://from-local");
}

#[test]
fn run_manifest_task_run_array_supports_env_file_directive_array_precedence() {
    let root = temp_workspace("run-array-env-value-step-env-file-array");
    let marker = root.join("env-value-step-env-file-array.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks]
api = [
  {{ env_file = [".env.local", ".env.test"] }},
  {{ env = "DATABASE_URL" }},
  {{ run = "sh -lc 'printf %s \"$DATABASE_URL\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );
    fs::write(
        root.join(".env.test"),
        "DATABASE_URL=postgres://from-test\n",
    )
    .expect("write .env.test");
    fs::write(
        root.join(".env.local"),
        "DATABASE_URL=postgres://from-local\n",
    )
    .expect("write .env.local");

    assert_task_output_equals(&root, "api", &marker, "postgres://from-local");
}

#[test]
fn run_manifest_task_run_array_errors_for_task_env_file_array_empty_entry() {
    let root = temp_workspace("run-array-env-value-task-env-file-array-empty-entry");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.api]
env_file = [".env.test", "   "]
run = [
  { env = "DATABASE_URL" },
  { run = "printf unreachable" }
]
"#,
    );
    let err = run_builtin_err(root, "api", &[]);
    assert_task_invocation_error_contains(err, &["task env_file[1] is invalid"]);
}
