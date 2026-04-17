use super::cases::assert_case_table;
use super::errors::{assert_invocation_error_contains, assert_task_lock_conflict};
use super::execution::run_manifest_task_with_cwd;
use super::harness::{
    create_workspace_dir, temp_workspace, write_catalog_tasks, write_manifest, write_root_manifest,
    EnvGuard,
};
use super::output::{
    assert_output_contains_all, assert_output_contains_derived, assert_output_excludes_all,
    assert_path_exists, assert_path_missing,
};
use super::runtime::{fs, thread, Duration, Path, PathBuf, RunnerError, TaskInvocation};

// ---------------------------------------------------------------------------
// Cases
// ---------------------------------------------------------------------------

pub(in crate::runner::tests) enum ManagedInvocation {
    Dev,
    DevWithRepo,
    TaskWithRepo(&'static str),
}

pub(in crate::runner::tests) struct ManagedOutputCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) invocation: ManagedInvocation,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
    pub(in crate::runner::tests) expected_absent: &'static [&'static str],
    pub(in crate::runner::tests) setup: fn(&Path),
}

pub(in crate::runner::tests) struct ManagedOutputDerivedCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) invocation: ManagedInvocation,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
    pub(in crate::runner::tests) expected_absent: &'static [&'static str],
    pub(in crate::runner::tests) expected_derived: fn(&Path) -> Vec<String>,
    pub(in crate::runner::tests) setup: fn(&Path),
}

pub(in crate::runner::tests) struct ManagedInvalidDefinitionCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) expected_task: &'static str,
    pub(in crate::runner::tests) expected_process: &'static str,
    pub(in crate::runner::tests) expected_detail_substring: Option<&'static str>,
}

pub(in crate::runner::tests) struct ManagedStreamBuiltinTestCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) suite: &'static str,
    pub(in crate::runner::tests) task_ref: &'static str,
}

pub(in crate::runner::tests) struct ManagedTaskRefInvalidCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) expected_reference: &'static str,
    pub(in crate::runner::tests) expected_detail: &'static str,
}

pub(in crate::runner::tests) struct ManagedProfileNotFoundCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) invocation: ManagedInvocation,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) setup: fn(&Path),
    pub(in crate::runner::tests) expected_task: &'static str,
    pub(in crate::runner::tests) expected_profile: &'static str,
    pub(in crate::runner::tests) expected_available: &'static [&'static str],
}

pub(in crate::runner::tests) struct ManagedNonZeroExitCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) invocation: ManagedInvocation,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) setup: fn(&Path),
    pub(in crate::runner::tests) expected_task: &'static str,
    pub(in crate::runner::tests) expected_profile: &'static str,
    pub(in crate::runner::tests) expected_processes: &'static [(&'static str, &'static str)],
}

pub(in crate::runner::tests) struct ManagedUnlockInvocationErrorCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct ManagedUnlockSuccessCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) lock_files: &'static [(&'static str, &'static str)],
    pub(in crate::runner::tests) removed_lock_files: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

pub(in crate::runner::tests) fn managed_tui_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))])
}

pub(in crate::runner::tests) fn managed_stream_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))])
}

pub(in crate::runner::tests) fn write_catalogs_with_tasks(
    root: &Path,
    catalogs: &[(&str, &[(&str, &str)])],
) {
    for (name, tasks) in catalogs {
        let dir = create_workspace_dir(root, name);
        write_catalog_tasks(&dir, Some(name), tasks);
    }
}

pub(in crate::runner::tests) fn write_catalog_a_and_catalog_c_dev_catalogs(root: &Path) {
    write_catalogs_with_tasks(
        root,
        &[
            ("catalog_a", &[("api", "printf catalog_a-api")]),
            ("catalog_c", &[("dev", "printf catalog_c-dev")]),
        ],
    );
}

pub(in crate::runner::tests) fn write_froyo_validate_catalog(root: &Path) {
    write_catalogs_with_tasks(root, &[("froyo", &[("validate", "printf froyo-validate")])]);
}

pub(in crate::runner::tests) fn write_managed_admin_profile_manifest(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "front", run = "vite dev", start = 2, tab = 2 },
  { name = "admin", run = "vite dev --config admin", start = 3, tab = 3 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "admin", run = "vite dev --config admin", start = 2, tab = 2 }
]
"#,
    );
}

pub(in crate::runner::tests) fn write_managed_stream_builtin_test_manifest(
    root: &Path,
    suite: &str,
    test_task_ref: &str,
    marker: &Path,
) {
    write_root_manifest(
        root,
        &format!(
            r#"[test.suites]
{} = "sh -lc 'printf called > \"{}\"'"

[tasks.dev]
mode = "tui"
concurrent = [{{ name = "tests", task = "{}" }}]
"#,
            suite,
            marker.display(),
            test_task_ref
        ),
    );
}

pub(in crate::runner::tests) fn write_managed_stream_profile_manifest(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "default-only", run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ name = "front-only", run = "printf front-ok" }]
"#,
    );
}

pub(in crate::runner::tests) fn write_managed_tui_dev_manifest(root: &Path, concurrent: &str) {
    write_root_manifest(
        root,
        &format!("[tasks.dev]\nmode = \"tui\"\nconcurrent = {concurrent}\n"),
    );
}

pub(in crate::runner::tests) fn write_managed_tui_dev_manifest_with_extra(
    root: &Path,
    concurrent: &str,
    extra_sections: &str,
) {
    write_root_manifest(
        root,
        &format!("[tasks.dev]\nmode = \"tui\"\nconcurrent = {concurrent}\n\n{extra_sections}\n"),
    );
}

pub(in crate::runner::tests) fn write_catalog_manifest_with_alias(
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

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

fn run_dev_with_manifest_error(workspace: &str, manifest: &str, context: &str) -> RunnerError {
    let root = temp_workspace(workspace);
    write_root_manifest(&root, manifest);
    run_dev_with_repo(&root, &[]).expect_err(context)
}

pub(in crate::runner::tests) fn assert_managed_process_invalid_definition(
    err: RunnerError,
    expected_task: &str,
    expected_process: &str,
    expected_detail_substring: Option<&str>,
) {
    match err {
        RunnerError::TaskManagedProcessInvalidDefinition {
            task,
            process,
            detail,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(process, expected_process);
            if let Some(detail_substring) = expected_detail_substring {
                assert!(detail.contains(detail_substring));
            }
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(in crate::runner::tests) fn assert_managed_invalid_definition_case_table(
    cases: &[ManagedInvalidDefinitionCase],
) {
    assert_case_table(cases.iter(), |case| {
        let err = run_dev_with_manifest_error(
            case.workspace,
            case.manifest,
            "invalid managed definition should fail",
        );
        assert_managed_process_invalid_definition(
            err,
            case.expected_task,
            case.expected_process,
            case.expected_detail_substring,
        );
    });
}

pub(in crate::runner::tests) fn assert_managed_profile_not_found(
    err: RunnerError,
    expected_task: &str,
    expected_profile: &str,
    expected_available: &[&str],
) {
    match err {
        RunnerError::TaskManagedProfileNotFound {
            task,
            profile,
            available,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(profile, expected_profile);
            assert_eq!(
                available,
                expected_available
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>()
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(in crate::runner::tests) fn assert_managed_task_ref_invalid_case_table(
    cases: &[ManagedTaskRefInvalidCase],
) {
    assert_case_table(cases.iter(), |case| {
        let err = run_dev_with_manifest_error(
            case.workspace,
            case.manifest,
            "invalid process task ref should fail",
        );
        assert_managed_task_reference_invalid(
            err,
            "dev",
            "tests",
            case.expected_reference,
            case.expected_detail,
        );
    });
}

pub(in crate::runner::tests) fn assert_managed_task_reference_invalid(
    err: RunnerError,
    expected_task: &str,
    expected_process: &str,
    expected_reference: &str,
    expected_detail_substring: &str,
) {
    match err {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task,
            process,
            reference,
            detail,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(process, expected_process);
            assert_eq!(reference, expected_reference);
            assert!(detail.contains(expected_detail_substring));
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(in crate::runner::tests) fn assert_managed_non_zero_exit(
    err: RunnerError,
    expected_task: &str,
    expected_profile: &str,
    expected_processes: &[(&str, &str)],
) {
    match err {
        RunnerError::TaskManagedNonZeroExit {
            task,
            profile,
            processes,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(profile, expected_profile);
            assert_eq!(
                processes,
                expected_processes
                    .iter()
                    .map(|(process, exit)| ((*process).to_owned(), (*exit).to_owned()))
                    .collect::<Vec<_>>()
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(in crate::runner::tests) fn assert_lock_conflict(
    err: RunnerError,
    expected_scope: &str,
    expected_remediation: &str,
) {
    assert_task_lock_conflict(err, expected_scope, expected_remediation);
}

// ---------------------------------------------------------------------------
// Runtime / invocation helpers
// ---------------------------------------------------------------------------

fn task_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

fn repo_scoped_args(root: &Path, args: &[&str]) -> Vec<String> {
    let mut full_args = vec!["--repo".to_owned(), root.display().to_string()];
    full_args.extend(task_args(args));
    full_args
}

fn run_named_task(root: &Path, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: task_args(args),
        },
        root.to_path_buf(),
    )
}

fn run_named_task_with_repo(root: &Path, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: repo_scoped_args(root, args),
        },
        root.to_path_buf(),
    )
}

pub(in crate::runner::tests) fn run_dev(root: &Path, args: &[&str]) -> Result<String, RunnerError> {
    run_named_task(root, "dev", args)
}

pub(in crate::runner::tests) fn run_dev_with_repo(
    root: &Path,
    args: &[&str],
) -> Result<String, RunnerError> {
    run_named_task_with_repo(root, "dev", args)
}

pub(in crate::runner::tests) fn run_unlock_with_repo(
    root: &Path,
    scopes: &[&str],
) -> Result<String, RunnerError> {
    run_named_task_with_repo(root, "unlock", scopes)
}

pub(in crate::runner::tests) fn run_task_with_repo(
    root: &Path,
    name: &str,
    args: &[&str],
) -> Result<String, RunnerError> {
    run_named_task_with_repo(root, name, args)
}

pub(in crate::runner::tests) fn run_managed_invocation(
    root: &Path,
    invocation: &ManagedInvocation,
    args: &[&str],
) -> Result<String, RunnerError> {
    match invocation {
        ManagedInvocation::Dev => run_dev(root, args),
        ManagedInvocation::DevWithRepo => run_dev_with_repo(root, args),
        ManagedInvocation::TaskWithRepo(task) => run_task_with_repo(root, task, args),
    }
}

pub(in crate::runner::tests) fn assert_live_dev_lock_conflict(
    root: &Path,
    warmup_ms: u64,
    expected_scope: &str,
    expected_remediation: &str,
) {
    let root_for_thread = root.to_path_buf();
    let join = thread::spawn(move || run_dev(&root_for_thread, &[]));

    std::thread::sleep(Duration::from_millis(warmup_ms));

    let err = run_dev(root, &[]).expect_err("second run should conflict on lock");
    assert_lock_conflict(err, expected_scope, expected_remediation);

    join.join()
        .expect("thread join")
        .expect("first run should complete");
}

// ---------------------------------------------------------------------------
// Lock helpers
// ---------------------------------------------------------------------------

fn workspace_root(workspace: &str) -> PathBuf {
    temp_workspace(workspace)
}

fn for_each_workspace_case<T>(
    cases: &[T],
    workspace_of: impl Fn(&T) -> &str,
    mut visit: impl FnMut(PathBuf, &T),
) {
    assert_case_table(cases.iter(), |case| {
        let root = workspace_root(workspace_of(case));
        visit(root, case);
    });
}

fn prepare_unlock_workspace(root: &Path, lock_files: Option<&[(&str, &str)]>) {
    match lock_files {
        Some(lock_files) => write_lock_files(root, lock_files),
        None => {
            ensure_locks_dir(root);
        }
    }
}

pub(in crate::runner::tests) fn ensure_locks_dir(root: &Path) -> PathBuf {
    let locks_dir = root.join(".effigy/locks");
    fs::create_dir_all(&locks_dir).expect("mkdir locks");
    locks_dir
}

pub(in crate::runner::tests) fn write_lock_files(root: &Path, files: &[(&str, &str)]) {
    let locks_dir = ensure_locks_dir(root);
    for (name, body) in files {
        fs::write(locks_dir.join(name), body).expect("write lock file");
    }
}

pub(in crate::runner::tests) fn assert_lock_files_missing(root: &Path, files: &[&str]) {
    let locks_dir = root.join(".effigy/locks");
    for name in files {
        assert_path_missing(&locks_dir.join(name), "lock file");
    }
}

pub(in crate::runner::tests) fn assert_unlock_invocation_error_case_table(
    cases: &[ManagedUnlockInvocationErrorCase],
) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |root, case| {
            prepare_unlock_workspace(&root, None);
            let err = run_unlock_with_repo(&root, case.args).expect_err("unlock should fail");
            assert_invocation_error_contains(err, case.expected);
        },
    );
}

pub(in crate::runner::tests) fn assert_unlock_success_case_table(
    cases: &[ManagedUnlockSuccessCase],
) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |root, case| {
            prepare_unlock_workspace(&root, Some(case.lock_files));
            let out = run_unlock_with_repo(&root, case.args).expect("unlock should run");
            assert_output_contains_all(&out, case.expected);
            assert_lock_files_missing(&root, case.removed_lock_files);
        },
    );
}

// ---------------------------------------------------------------------------
// Case-table runners
// ---------------------------------------------------------------------------

fn setup_workspace_with(workspace: &str, setup: fn(&Path)) -> PathBuf {
    let root = workspace_root(workspace);
    setup(&root);
    root
}

fn run_managed_case(
    workspace: &str,
    setup: fn(&Path),
    invocation: &ManagedInvocation,
    args: &[&str],
) -> (PathBuf, Result<String, RunnerError>) {
    let root = setup_workspace_with(workspace, setup);
    let result = run_managed_invocation(&root, invocation, args);
    (root, result)
}

fn assert_managed_case_table<T>(
    cases: &[T],
    workspace_of: impl Fn(&T) -> &str,
    setup_of: impl Fn(&T) -> fn(&Path),
    invocation_of: impl Fn(&T) -> &ManagedInvocation,
    args_of: impl Fn(&T) -> &[&str],
    mut assert_case: impl FnMut(&T, PathBuf, Result<String, RunnerError>),
) {
    assert_case_table(cases.iter(), |case| {
        let (root, result) = run_managed_case(
            workspace_of(case),
            setup_of(case),
            invocation_of(case),
            args_of(case),
        );
        assert_case(case, root, result);
    });
}

fn assert_managed_output_contract(out: &str, expected: &[&str], expected_absent: &[&str]) {
    assert_output_contains_all(out, expected);
    assert_output_excludes_all(out, expected_absent);
}

pub(in crate::runner::tests) fn assert_managed_output_case_table(cases: &[ManagedOutputCase]) {
    assert_managed_case_table(
        cases,
        |case| case.workspace,
        |case| case.setup,
        |case| &case.invocation,
        |case| case.args,
        |case, _root, out| {
            let out = out.expect("managed plan should render");
            assert_managed_output_contract(&out, case.expected, case.expected_absent);
        },
    );
}

pub(in crate::runner::tests) fn assert_managed_output_derived_case_table(
    cases: &[ManagedOutputDerivedCase],
) {
    assert_managed_case_table(
        cases,
        |case| case.workspace,
        |case| case.setup,
        |case| &case.invocation,
        |case| case.args,
        |case, root, out| {
            let out = out.expect("managed plan should render");
            assert_managed_output_contract(&out, case.expected, case.expected_absent);
            assert_output_contains_derived(&out, (case.expected_derived)(&root));
        },
    );
}

pub(in crate::runner::tests) fn assert_managed_profile_not_found_case_table(
    cases: &[ManagedProfileNotFoundCase],
) {
    assert_managed_case_table(
        cases,
        |case| case.workspace,
        |case| case.setup,
        |case| &case.invocation,
        |case| case.args,
        |case, _root, result| {
            let err = result.expect_err("expected missing managed profile error");
            assert_managed_profile_not_found(
                err,
                case.expected_task,
                case.expected_profile,
                case.expected_available,
            );
        },
    );
}

pub(in crate::runner::tests) fn assert_managed_non_zero_exit_case_table(
    cases: &[ManagedNonZeroExitCase],
) {
    assert_managed_case_table(
        cases,
        |case| case.workspace,
        |case| case.setup,
        |case| &case.invocation,
        |case| case.args,
        |case, _root, result| {
            let err = result.expect_err("expected managed non-zero exit error");
            assert_managed_non_zero_exit(
                err,
                case.expected_task,
                case.expected_profile,
                case.expected_processes,
            );
        },
    );
}

pub(in crate::runner::tests) fn assert_managed_stream_builtin_test_case_table(
    cases: &[ManagedStreamBuiltinTestCase],
) {
    for_each_workspace_case(
        cases,
        |case| case.workspace,
        |root, case| {
            let marker = root.join("builtin-test-called.log");
            write_managed_stream_builtin_test_manifest(&root, case.suite, case.task_ref, &marker);

            let out = run_dev(&root, &["default"])
                .expect("run managed stream with builtin profile entry");
            assert_output_contains_all(&out, &["Managed Task Runtime", "root: ok"]);
            assert_path_exists(&marker, "built-in test task ref marker");
        },
    );
}

// ---------------------------------------------------------------------------
// Concurrent-configuration helpers (promoted from the former nested prelude)
// ---------------------------------------------------------------------------

pub(in crate::runner::tests) fn write_ranked_task_ref_manifest(
    root: &Path,
    jobs_start_after_ms: Option<u32>,
) {
    let jobs_delay = jobs_start_after_ms
        .map(|ms| format!(", start_after_ms = {ms}"))
        .unwrap_or_default();
    write_root_manifest(
        root,
        &format!(
            r#"[tasks.dev]
mode = "tui"
concurrent = [
  {{ task = "catalog_a/api", start = 1, tab = 3 }},
  {{ task = "catalog_a/jobs", start = 2, tab = 4{} }},
  {{ task = "catalog_c/dev", start = 3, tab = 2 }},
  {{ task = "catalog_b/dev", start = 4, tab = 1 }}
]
"#,
            jobs_delay
        ),
    );
}

pub(in crate::runner::tests) fn write_ranked_name_manifest(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api", start = 1, tab = 3 },
  { name = "jobs", run = "printf jobs", start = 2, tab = 4 },
  { name = "catalog_c", run = "printf catalog_c", start = 3, tab = 2 },
  { name = "catalog_b", run = "printf catalog_b", start = 4, tab = 1 }
]
"#,
    );
}

pub(in crate::runner::tests) fn write_ranked_catalog_tasks(root: &Path) {
    write_catalogs_with_tasks(
        root,
        &[
            (
                "catalog_a",
                &[
                    ("api", "printf catalog_a-api"),
                    ("jobs", "printf catalog_a-jobs"),
                ] as &[(&str, &str)],
            ),
            (
                "catalog_c",
                &[("dev", "printf catalog_c-dev")] as &[(&str, &str)],
            ),
            (
                "catalog_b",
                &[("dev", "printf catalog_b-dev")] as &[(&str, &str)],
            ),
        ],
    );
}
