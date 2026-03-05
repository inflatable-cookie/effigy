pub(super) use super::super::prelude::*;

pub(super) struct RunArrayRuntimeFlowCase {
    pub(super) workspace: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) marker_rel: &'static str,
    pub(super) dag_max_parallel: Option<u32>,
    pub(super) expected_marker: &'static [&'static str],
    pub(super) start_markers: &'static [&'static str],
    pub(super) end_markers: &'static [&'static str],
    pub(super) min_elapsed_ms: Option<u64>,
    pub(super) setup: fn(&Path, &Path),
}

pub(super) struct RunArrayRuntimeErrorCase {
    pub(super) workspace: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) marker_rel: Option<&'static str>,
    pub(super) expected_marker: &'static [&'static str],
    pub(super) expected_code: Option<Option<i32>>,
    pub(super) setup: fn(&Path, Option<&Path>),
}

fn flow_workspace(case: &RunArrayRuntimeFlowCase) -> (PathBuf, PathBuf) {
    let root = temp_workspace(case.workspace);
    let marker = root.join(case.marker_rel);
    (case.setup)(&root, &marker);
    (root, marker)
}

fn error_workspace(case: &RunArrayRuntimeErrorCase) -> (PathBuf, Option<PathBuf>) {
    let root = temp_workspace(case.workspace);
    let marker = case.marker_rel.map(|relative| root.join(relative));
    (case.setup)(&root, marker.as_deref());
    (root, marker)
}

fn normalize_project_placeholder(input: &str) -> String {
    input.replace("{{project}}", "{project}")
}

fn render_single_env_capture_run(marker: &Path, env_var: &str) -> String {
    let shell_var = format!("${env_var}");
    format!(
        "sh -lc 'printf %s \\\"{shell_var}\\\" > \\\"{}\\\"'",
        marker.display()
    )
}

fn render_dual_env_capture_run(marker: &Path, env_vars: (&str, &str)) -> String {
    let shell_var1 = format!("${}", env_vars.0);
    let shell_var2 = format!("${}", env_vars.1);
    format!(
        "sh -lc 'printf \\\"%s|%s\\\" \\\"{shell_var1}\\\" \\\"{shell_var2}\\\" > \\\"{}\\\"'",
        marker.display()
    )
}

fn append_manifest_prelude(manifest: &mut String, env_prelude: Option<&str>) {
    if let Some(prelude) = env_prelude {
        manifest.push_str(&normalize_project_placeholder(prelude));
        manifest.push_str("\n\n");
    }
}

fn write_root_api_manifest(
    root: &Path,
    env_prelude: Option<&str>,
    env_step_exprs: &[&str],
    run: &str,
) {
    let mut manifest = String::new();
    append_manifest_prelude(&mut manifest, env_prelude);
    manifest.push_str("[tasks]\napi = [\n");
    for step in env_step_exprs {
        let step = normalize_project_placeholder(step);
        manifest.push_str(&format!("  {{ env = {step} }},\n"));
    }
    manifest.push_str(&format!("  {{ run = \"{run}\" }}\n"));
    manifest.push_str("]\n");
    write_manifest(&root.join("effigy.toml"), &manifest);
}

fn write_task_api_manifest(
    root: &Path,
    task_env_file_expr: Option<&str>,
    step_env_file_expr: Option<&str>,
    env_step_expr: &str,
    run: &str,
) {
    let mut manifest = String::new();
    manifest.push_str("[tasks.api]\n");
    if let Some(task_env_file_expr) = task_env_file_expr.map(normalize_project_placeholder) {
        manifest.push_str(&format!("env_file = {task_env_file_expr}\n"));
    }
    manifest.push_str("run = [\n");
    if let Some(step_env_file_expr) = step_env_file_expr.map(normalize_project_placeholder) {
        manifest.push_str(&format!("  {{ env_file = {step_env_file_expr} }},\n"));
    }
    let env_step_expr = normalize_project_placeholder(env_step_expr);
    manifest.push_str(&format!("  {{ env = {env_step_expr} }},\n"));
    manifest.push_str(&format!("  {{ run = \"{run}\" }}\n"));
    manifest.push_str("]\n");
    write_manifest(&root.join("effigy.toml"), &manifest);
}

fn write_catalog_api_manifest(
    root: &Path,
    catalog_dir: &str,
    alias: &str,
    env_step_expr: &str,
    run: &str,
) {
    let env_step_expr = normalize_project_placeholder(env_step_expr);
    write_catalog_manifest_with_alias(
        root,
        catalog_dir,
        alias,
        &format!("[tasks]\napi = [\n  {{ env = {env_step_expr} }},\n  {{ run = \"{run}\" }}\n]"),
    );
}

pub(super) fn dag_max_parallel_env(parallelism: u32) -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_DAG_MAX_PARALLEL", Some(parallelism.to_string()))])
}

pub(super) fn write_env_files(root: &Path, entries: &[(&str, &str)]) {
    for (relative, body) in entries {
        fs::write(root.join(relative), body).expect("write env file");
    }
}

pub(super) fn write_catalog_manifest_with_alias(
    root: &Path,
    catalog_dir: &str,
    alias: &str,
    body: &str,
) {
    let dir = create_workspace_dir(root, catalog_dir);
    write_manifest(
        &dir.join("effigy.toml"),
        &format!("[catalog]\nalias = \"{alias}\"\n{body}\n"),
    );
}

pub(super) fn write_catalog_api_single_env_capture_manifest(
    root: &Path,
    catalog_dir: &str,
    alias: &str,
    env_step_expr: &str,
    env_var: &str,
    marker: &Path,
) {
    let run = render_single_env_capture_run(marker, env_var);
    write_catalog_api_manifest(root, catalog_dir, alias, env_step_expr, &run);
}

pub(super) fn write_catalog_api_unreachable_manifest(
    root: &Path,
    catalog_dir: &str,
    alias: &str,
    env_step_expr: &str,
) {
    write_catalog_api_manifest(
        root,
        catalog_dir,
        alias,
        env_step_expr,
        "printf unreachable",
    );
}

pub(super) fn write_root_api_single_env_capture_manifest(
    root: &Path,
    marker: &Path,
    env_prelude: Option<&str>,
    env_step_expr: &str,
    env_var: &str,
) {
    let run = render_single_env_capture_run(marker, env_var);
    write_root_api_manifest(root, env_prelude, &[env_step_expr], &run);
}

pub(super) fn write_root_api_dual_env_capture_manifest(
    root: &Path,
    marker: &Path,
    env_prelude: Option<&str>,
    env_step_exprs: &[&str],
    env_vars: (&str, &str),
) {
    let run = render_dual_env_capture_run(marker, env_vars);
    write_root_api_manifest(root, env_prelude, env_step_exprs, &run);
}

pub(super) fn write_task_api_env_capture_manifest(
    root: &Path,
    marker: &Path,
    task_env_file_expr: Option<&str>,
    step_env_file_expr: Option<&str>,
    env_step_expr: &str,
    env_var: &str,
) {
    let run = render_single_env_capture_run(marker, env_var);
    write_task_api_manifest(
        root,
        task_env_file_expr,
        step_env_file_expr,
        env_step_expr,
        &run,
    );
}

pub(super) fn write_task_api_env_unreachable_manifest(
    root: &Path,
    task_env_file_expr: Option<&str>,
    step_env_file_expr: Option<&str>,
    env_step_expr: &str,
) {
    write_task_api_manifest(
        root,
        task_env_file_expr,
        step_env_file_expr,
        env_step_expr,
        "printf unreachable",
    );
}

pub(super) fn read_marker_text(marker: &Path) -> String {
    fs::read_to_string(marker).expect("read marker")
}

pub(super) fn read_marker_lines(marker: &Path) -> Vec<String> {
    read_marker_text(marker)
        .lines()
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn assert_marker_contains_all(marker: &Path, expected: &[&str]) {
    let body = read_marker_text(marker);
    for needle in expected {
        assert!(
            body.contains(needle),
            "expected marker output to contain `{needle}`, got:\n{body}"
        );
    }
}

pub(super) fn assert_started_before_any_end(
    lines: &[String],
    start_markers: &[&str],
    end_markers: &[&str],
) {
    assert!(
        start_markers
            .iter()
            .all(|marker| lines.iter().any(|line| line == marker)),
        "expected start markers {start_markers:?}, got lines={lines:?}"
    );
    assert!(
        end_markers
            .iter()
            .all(|marker| lines.iter().any(|line| line == marker)),
        "expected end markers {end_markers:?}, got lines={lines:?}"
    );
    let first_end_idx = lines
        .iter()
        .position(|line| end_markers.iter().any(|marker| line == marker))
        .expect("expected at least one end marker");
    let starts_before_end = lines[..first_end_idx]
        .iter()
        .filter(|line| start_markers.iter().any(|marker| line == marker))
        .count();
    assert_eq!(
        starts_before_end,
        start_markers.len(),
        "expected all start markers before first end, lines={lines:?}"
    );
}

pub(super) fn assert_elapsed_at_least(elapsed: Duration, min_ms: u64) {
    assert!(
        elapsed >= Duration::from_millis(min_ms),
        "expected elapsed >= {min_ms}ms, elapsed={elapsed:?}"
    );
}

pub(super) fn assert_run_array_runtime_flow_case_table(cases: &[RunArrayRuntimeFlowCase]) {
    assert_case_table(cases.iter(), |case| {
        let (root, marker) = flow_workspace(case);
        let _env = case.dag_max_parallel.map(dag_max_parallel_env);

        let start = Instant::now();
        let _ = run_validate_ok(&root, case.args);
        let elapsed = start.elapsed();

        assert_marker_contains_all(&marker, case.expected_marker);
        if !case.start_markers.is_empty() && !case.end_markers.is_empty() {
            let lines = read_marker_lines(&marker);
            assert_started_before_any_end(&lines, case.start_markers, case.end_markers);
        }
        if let Some(min_elapsed_ms) = case.min_elapsed_ms {
            assert_elapsed_at_least(elapsed, min_elapsed_ms);
        }
    });
}

pub(super) fn assert_run_array_runtime_error_case_table(cases: &[RunArrayRuntimeErrorCase]) {
    assert_case_table(cases.iter(), |case| {
        let (root, marker) = error_workspace(case);

        let err = run_validate_err(&root, case.args);
        assert_task_command_failure_code(err, case.expected_code);
        if let Some(marker) = marker {
            assert_marker_contains_all(&marker, case.expected_marker);
        }
    });
}

pub(super) fn expected_cargo_paths(root: &Path) -> String {
    let canonical_root = fs::canonicalize(root).expect("canonicalize root");
    format!(
        "{}/.cargo/home|{}/.cargo/target",
        canonical_root.display(),
        canonical_root.display()
    )
}
