use super::*;

pub(crate) fn run_dev(root: &PathBuf, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root.clone(),
    )
}

pub(crate) fn run_dev_with_repo(root: &PathBuf, args: &[&str]) -> Result<String, RunnerError> {
    let mut full_args = vec!["--repo".to_owned(), root.display().to_string()];
    full_args.extend(args.iter().map(|arg| (*arg).to_owned()));
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: full_args,
        },
        root.clone(),
    )
}

pub(crate) fn run_unlock_with_repo(root: &PathBuf, scopes: &[&str]) -> Result<String, RunnerError> {
    let mut args = vec!["--repo".to_owned(), root.display().to_string()];
    args.extend(scopes.iter().map(|scope| (*scope).to_owned()));
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "unlock".to_owned(),
            args,
        },
        root.clone(),
    )
}

pub(crate) fn run_task_with_repo(
    root: &PathBuf,
    name: &str,
    args: &[&str],
) -> Result<String, RunnerError> {
    let mut full_args = vec!["--repo".to_owned(), root.display().to_string()];
    full_args.extend(args.iter().map(|arg| (*arg).to_owned()));
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: full_args,
        },
        root.clone(),
    )
}

pub(crate) fn assert_run_dev_with_repo_contains(root: &PathBuf, args: &[&str], expected: &[&str]) {
    let out = run_dev_with_repo(root, args).expect("managed plan should render");
    assert_contains_all(&out, expected);
}
