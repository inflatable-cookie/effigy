use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::thread;

use tempfile::TempDir;

use super::{
    ensure_effigy_ignored_in_git_root, ensure_local_overlay_ignored_in_git_root,
    git_local_exclude_path, pattern_already_listed,
};

/// Historical helper: appended the pattern to `.gitignore`. Kept as the
/// clean-tree oracle's failing baseline.
fn legacy_ensure_pattern_ignored_in_gitignore(
    repo_root: &Path,
    append_line: &str,
    accepted_aliases: &[&str],
) -> io::Result<bool> {
    if !repo_root.join(".git").is_dir() {
        return Ok(false);
    }
    let gitignore_path = repo_root.join(".gitignore");
    let existing = match fs::read_to_string(&gitignore_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    if existing
        .lines()
        .map(str::trim)
        .any(|line| accepted_aliases.contains(&line))
    {
        return Ok(false);
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(append_line);
    updated.push('\n');
    fs::write(gitignore_path, updated)?;
    Ok(true)
}

fn git(cwd: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "user.name=Effigy Test",
            "-c",
            "user.email=effigy@example.invalid",
        ])
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Effigy Test")
        .env("GIT_AUTHOR_EMAIL", "effigy@example.invalid")
        .env("GIT_COMMITTER_NAME", "Effigy Test")
        .env("GIT_COMMITTER_EMAIL", "effigy@example.invalid")
        .output()
        .unwrap_or_else(|error| panic!("git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn git_init_with_tracked_manifest(root: &Path) {
    git(root, &["init", "--quiet"]);
    fs::write(root.join("effigy.toml"), "[tasks]\nprobe = \"true\"\n").expect("manifest");
    git(root, &["add", "effigy.toml"]);
    git(root, &["commit", "--quiet", "-m", "init"]);
}

fn porcelain(root: &Path) -> String {
    String::from_utf8_lossy(&git(root, &["status", "--porcelain"]).stdout).into_owned()
}

fn check_ignore(root: &Path, path: &str) -> bool {
    Command::new("git")
        .args(["-c", "commit.gpgsign=false", "check-ignore", "-q", path])
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .expect("git check-ignore")
        .success()
}

fn exclude_contents(root: &Path) -> String {
    fs::read_to_string(git_local_exclude_path(root)).expect("read exclude")
}

#[test]
fn legacy_gitignore_helper_fails_the_clean_tree_oracle() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    git_init_with_tracked_manifest(root);
    assert!(!root.join(".gitignore").exists());
    assert_eq!(porcelain(root), "");

    let changed =
        legacy_ensure_pattern_ignored_in_gitignore(root, ".effigy", &[".effigy", ".effigy/"])
            .expect("legacy ignore");
    assert!(changed);
    assert_eq!(
        fs::read_to_string(root.join(".gitignore")).expect("read"),
        ".effigy\n"
    );
    let status = porcelain(root);
    assert!(
        status.contains(".gitignore"),
        "legacy helper must dirty the tree: {status:?}"
    );
}

#[test]
fn implicit_cache_state_keeps_a_clean_tree_without_creating_gitignore() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    git_init_with_tracked_manifest(root);
    assert!(!root.join(".gitignore").exists());

    fs::create_dir_all(root.join(".effigy/cache")).expect("cache dir");
    fs::write(root.join(".effigy/cache/store.json"), "{}\n").expect("cache file");
    fs::create_dir_all(root.join(".effigy/locks")).expect("locks dir");
    fs::write(root.join(".effigy/locks/task-probe.lock"), "lock\n").expect("lock file");

    let dirty_before = porcelain(root);
    assert!(
        dirty_before.contains(".effigy"),
        "unignored runtime state must show: {dirty_before:?}"
    );

    let changed = ensure_effigy_ignored_in_git_root(root).expect("ignore");
    assert!(changed);
    assert!(!root.join(".gitignore").exists());
    assert!(exclude_contents(root)
        .lines()
        .any(|line| line.trim() == ".effigy"));
    assert!(check_ignore(root, ".effigy/cache/store.json"));
    assert!(check_ignore(root, ".effigy/locks/task-probe.lock"));
    assert_eq!(porcelain(root), "");
}

#[test]
fn tracked_gitignore_bytes_stay_identical_whether_or_not_they_cover_the_pattern() {
    for (label, gitignore) in [("covers", "target\n.effigy\n"), ("unrelated", "target\n")] {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();
        git_init_with_tracked_manifest(root);
        fs::write(root.join(".gitignore"), gitignore).expect("gitignore");
        git(root, &["add", ".gitignore"]);
        git(root, &["commit", "--quiet", "-m", label]);
        let before = fs::read(root.join(".gitignore")).expect("read");

        ensure_effigy_ignored_in_git_root(root).expect("ignore");

        let after = fs::read(root.join(".gitignore")).expect("reread");
        assert_eq!(after, before, "{label}");
        assert_eq!(after, gitignore.as_bytes(), "{label}");
        assert!(
            check_ignore(root, ".effigy"),
            "{label}: git check-ignore .effigy failed"
        );
    }
}

#[test]
fn skips_non_git_roots_without_creating_git_or_gitignore() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();

    let changed = ensure_effigy_ignored_in_git_root(root).expect("ignore");

    assert!(!changed);
    assert!(!root.join(".git").exists());
    assert!(!root.join(".gitignore").exists());
}

#[test]
fn appends_to_exclude_without_duplicate_and_preserves_existing_rules() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    fs::create_dir(root.join(".git")).expect("git dir");
    let exclude = root.join(".git/info/exclude");
    fs::create_dir_all(exclude.parent().expect("info")).expect("info");
    fs::write(&exclude, "# keep me\n*.orig").expect("seed exclude");

    let changed = ensure_effigy_ignored_in_git_root(root).expect("ignore");
    let second = ensure_effigy_ignored_in_git_root(root).expect("ignore again");

    assert!(changed);
    assert!(!second);
    assert!(!root.join(".gitignore").exists());
    assert_eq!(
        fs::read_to_string(&exclude).expect("read"),
        "# keep me\n*.orig\n.effigy\n"
    );
}

#[test]
fn local_overlay_is_appended_once_to_exclude() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    fs::create_dir(root.join(".git")).expect("git dir");

    let changed = ensure_local_overlay_ignored_in_git_root(root).expect("ignore");
    let again = ensure_local_overlay_ignored_in_git_root(root).expect("ignore again");

    assert!(changed);
    assert!(!again);
    assert!(!root.join(".gitignore").exists());
    assert_eq!(
        fs::read_to_string(root.join(".git/info/exclude")).expect("read"),
        "effigy.local.toml\n"
    );
}

#[test]
fn local_overlay_skips_when_alias_present() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    fs::create_dir(root.join(".git")).expect("git dir");
    fs::create_dir_all(root.join(".git/info")).expect("info");
    fs::write(root.join(".git/info/exclude"), "/effigy.local.toml\n").expect("seed");

    let changed = ensure_local_overlay_ignored_in_git_root(root).expect("ignore");

    assert!(!changed);
    assert_eq!(
        fs::read_to_string(root.join(".git/info/exclude")).expect("read"),
        "/effigy.local.toml\n"
    );
}

#[test]
fn linked_worktree_writes_common_exclude_and_leaves_working_trees_untouched() {
    let tmp = TempDir::new().expect("tempdir");
    let primary = tmp.path().join("primary");
    fs::create_dir_all(&primary).expect("primary");
    git_init_with_tracked_manifest(&primary);
    git(&primary, &["worktree", "add", "--quiet", "../linked"]);
    let linked = tmp.path().join("linked");
    assert!(
        linked.join(".git").is_file(),
        "linked checkout uses a gitfile"
    );

    fs::create_dir_all(linked.join(".effigy/cache")).expect("runtime");
    fs::write(linked.join(".effigy/cache/store.json"), "{}\n").expect("cache");

    let changed = ensure_effigy_ignored_in_git_root(&linked).expect("ignore");
    assert!(changed);

    assert!(!linked.join(".gitignore").exists());
    assert!(!primary.join(".gitignore").exists());
    let exclude = primary.join(".git/info/exclude");
    assert!(
        fs::read_to_string(&exclude)
            .expect("common exclude")
            .lines()
            .any(|line| line.trim() == ".effigy"),
        "shared exclude at {}",
        exclude.display()
    );
    assert!(!linked.join(".git/info").exists());
    assert!(check_ignore(&linked, ".effigy/cache/store.json"));
    assert_eq!(porcelain(&linked), "");
    assert_eq!(porcelain(&primary), "");
}

#[test]
fn concurrent_writes_preserve_rules_and_avoid_duplicates() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().to_path_buf();
    fs::create_dir(root.join(".git")).expect("git dir");
    fs::create_dir_all(root.join(".git/info")).expect("info");
    fs::write(root.join(".git/info/exclude"), "# shared\n*.tmp\n").expect("seed");
    let root = Arc::new(root);

    let workers: Vec<_> = (0..8)
        .map(|_| {
            let root = Arc::clone(&root);
            thread::spawn(move || ensure_effigy_ignored_in_git_root(&root).expect("ignore"))
        })
        .collect();
    let results: Vec<bool> = workers
        .into_iter()
        .map(|worker| worker.join().expect("join"))
        .collect();

    assert_eq!(results.iter().filter(|changed| **changed).count(), 1);
    let contents = fs::read_to_string(root.join(".git/info/exclude")).expect("read");
    assert!(contents.starts_with("# shared\n*.tmp\n"));
    assert_eq!(contents.matches(".effigy\n").count(), 1);
    assert!(!root.join(".gitignore").exists());
}

#[test]
fn invalid_gitfile_fails_without_worktree_writes() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    fs::write(root.join(".git"), "gitdir: /nope/does/not/exist\n").expect("pointer");
    fs::write(root.join("tracked.txt"), "keep\n").expect("tracked");

    let error = ensure_effigy_ignored_in_git_root(root).expect_err("invalid marker");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(!root.join(".gitignore").exists());
    assert!(!root.join(".git/info").exists());
    assert_eq!(git_local_exclude_path(root), root.join(".git"));
}

#[test]
fn gitfile_pointing_at_a_non_git_directory_fails_without_writing_there() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().join("checkout");
    let decoy = tmp.path().join("decoy");
    fs::create_dir_all(&root).expect("checkout");
    fs::create_dir_all(&decoy).expect("decoy");
    fs::write(decoy.join("not-git"), "nope\n").expect("decoy file");
    fs::write(root.join(".git"), format!("gitdir: {}\n", decoy.display())).expect("pointer");

    let error = ensure_effigy_ignored_in_git_root(&root).expect_err("decoy");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(!decoy.join("info").exists());
    assert!(!root.join(".gitignore").exists());
    assert_eq!(git_local_exclude_path(&root), root.join(".git"));
}

#[test]
fn gitfile_pointing_at_a_bare_admin_dir_is_refused_without_writing_there() {
    let tmp = TempDir::new().expect("tempdir");
    let checkout = tmp.path().join("checkout");
    let bare = tmp.path().join("bare.git");
    fs::create_dir_all(&checkout).expect("checkout");
    git(tmp.path(), &["init", "--quiet", "--bare", "bare.git"]);
    fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", bare.display()),
    )
    .expect("gitfile");
    fs::write(checkout.join("tracked.txt"), "keep\n").expect("tracked");
    let exclude = bare.join("info/exclude");
    let before = fs::read(&exclude).unwrap_or_default();
    let status = Command::new("git")
        .args(["-c", "commit.gpgsign=false", "status", "--porcelain"])
        .current_dir(&checkout)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git status");
    assert!(
        !status.status.success(),
        "gitfile → bare is not a work tree"
    );

    let error = ensure_effigy_ignored_in_git_root(&checkout).expect_err("bare gitfile");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(&exclude).unwrap_or_default(), before);
    assert!(!checkout.join(".gitignore").exists());
    assert_eq!(git_local_exclude_path(&checkout), checkout.join(".git"));
}

#[test]
fn gitfile_pointing_at_last_wins_bare_config_is_refused_without_writing_there() {
    let tmp = TempDir::new().expect("tempdir");
    let checkout = tmp.path().join("checkout");
    let bare = tmp.path().join("bare.git");
    fs::create_dir_all(&checkout).expect("checkout");
    git(tmp.path(), &["init", "--quiet", "--bare", "bare.git"]);
    let config = bare.join("config");
    let existing = fs::read_to_string(&config).expect("bare config");
    fs::write(
        &config,
        format!("{existing}[core]\n\tbare = false\n\tbare = true\n"),
    )
    .expect("duplicate core.bare");
    fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", bare.display()),
    )
    .expect("gitfile");
    let exclude = bare.join("info/exclude");
    let before = fs::read(&exclude).unwrap_or_default();
    let status = Command::new("git")
        .args(["-c", "commit.gpgsign=false", "status", "--porcelain"])
        .current_dir(&checkout)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git status");
    assert!(
        !status.status.success(),
        "last-wins core.bare=true is not a work tree: {}",
        String::from_utf8_lossy(&status.stderr)
    );

    let error = ensure_effigy_ignored_in_git_root(&checkout).expect_err("last-wins bare");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(&exclude).unwrap_or_default(), before);
    assert!(!checkout.join(".gitignore").exists());
    assert_eq!(git_local_exclude_path(&checkout), checkout.join(".git"));
}

#[cfg(unix)]
#[test]
fn git_symlink_to_a_bare_admin_dir_is_refused_without_writing_there() {
    let tmp = TempDir::new().expect("tempdir");
    let checkout = tmp.path().join("checkout");
    let bare = tmp.path().join("bare.git");
    fs::create_dir_all(&checkout).expect("checkout");
    git(tmp.path(), &["init", "--quiet", "--bare", "bare.git"]);
    std::os::unix::fs::symlink(&bare, checkout.join(".git")).expect("symlink .git");
    fs::write(checkout.join("tracked.txt"), "keep\n").expect("tracked");
    let exclude = bare.join("info/exclude");
    let before = fs::read(&exclude).unwrap_or_default();
    let status = Command::new("git")
        .args(["-c", "commit.gpgsign=false", "status", "--porcelain"])
        .current_dir(&checkout)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git status");
    assert!(
        !status.status.success(),
        "symlink .git → bare is not a work tree: {}",
        String::from_utf8_lossy(&status.stderr)
    );

    let error = ensure_effigy_ignored_in_git_root(&checkout).expect_err("symlink bare");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(&exclude).unwrap_or_default(), before);
    assert!(!checkout.join(".gitignore").exists());
    assert_eq!(git_local_exclude_path(&checkout), checkout.join(".git"));
}

#[cfg(unix)]
#[test]
fn git_symlink_to_a_non_bare_git_dir_still_registers_exclude() {
    let tmp = TempDir::new().expect("tempdir");
    let primary = tmp.path().join("primary");
    fs::create_dir_all(&primary).expect("primary");
    git_init_with_tracked_manifest(&primary);
    let checkout = tmp.path().join("checkout");
    fs::create_dir_all(&checkout).expect("checkout");
    std::os::unix::fs::symlink(primary.join(".git"), checkout.join(".git")).expect("symlink .git");

    let changed = ensure_effigy_ignored_in_git_root(&checkout).expect("ignore");
    assert!(changed);
    assert!(!checkout.join(".gitignore").exists());
    let exclude = primary.join(".git/info/exclude");
    assert!(
        fs::read_to_string(&exclude)
            .expect("shared exclude")
            .lines()
            .any(|line| line.trim() == ".effigy"),
        "exclude at {}",
        exclude.display()
    );
}

#[test]
fn separate_git_dir_still_registers_common_exclude() {
    let tmp = TempDir::new().expect("tempdir");
    let checkout = tmp.path().join("checkout");
    let admin = tmp.path().join("admin.git");
    fs::create_dir_all(&checkout).expect("checkout");
    git(
        &checkout,
        &[
            "init",
            "--quiet",
            "--separate-git-dir",
            admin.to_str().expect("utf8 admin"),
        ],
    );
    fs::write(checkout.join("effigy.toml"), "[tasks]\nprobe = \"true\"\n").expect("manifest");
    git(&checkout, &["add", "effigy.toml"]);
    git(&checkout, &["commit", "--quiet", "-m", "init"]);
    assert!(checkout.join(".git").is_file());

    let changed = ensure_effigy_ignored_in_git_root(&checkout).expect("ignore");
    assert!(changed);
    assert!(!checkout.join(".gitignore").exists());
    let exclude = admin.join("info/exclude");
    assert!(
        fs::read_to_string(&exclude)
            .expect("admin exclude")
            .lines()
            .any(|line| line.trim() == ".effigy"),
        "separate-git-dir exclude at {}",
        exclude.display()
    );
    assert!(check_ignore(&checkout, ".effigy"));
    assert_eq!(porcelain(&checkout), "");
}

#[test]
fn bare_repo_root_is_a_non_working_tree_no_op() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    git(root, &["init", "--quiet", "--bare"]);

    let changed = ensure_effigy_ignored_in_git_root(root).expect("bare");
    assert!(!changed);
    assert!(!root.join(".gitignore").exists());
    let exclude = root.join("info/exclude");
    if exclude.exists() {
        let contents = fs::read_to_string(&exclude).expect("bare exclude");
        assert!(
            !contents.lines().any(|line| line.trim() == ".effigy"),
            "bare repo must not gain a runtime exclude: {contents:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn read_only_git_metadata_fails_without_gitignore_fallback() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    fs::create_dir(root.join(".git")).expect("git dir");
    let git_dir = root.join(".git");
    let original = fs::metadata(&git_dir).expect("meta").permissions();
    fs::set_permissions(&git_dir, fs::Permissions::from_mode(0o555)).expect("chmod");
    struct Restore {
        path: PathBuf,
        mode: u32,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.mode));
        }
    }
    let _restore = Restore {
        path: git_dir.clone(),
        mode: original.mode(),
    };

    let error = ensure_effigy_ignored_in_git_root(root).expect_err("readonly");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(!root.join(".gitignore").exists());
}

#[cfg(unix)]
#[test]
fn symlink_info_dir_escaping_git_admin_is_refused() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().join("repo");
    let escape = tmp.path().join("escape");
    fs::create_dir_all(&root).expect("repo");
    fs::create_dir_all(&escape).expect("escape");
    fs::create_dir(root.join(".git")).expect("git dir");
    std::os::unix::fs::symlink(&escape, root.join(".git/info")).expect("symlink info");

    let error = ensure_effigy_ignored_in_git_root(&root).expect_err("escape");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(fs::read_dir(&escape).expect("escape dir").next().is_none());
    assert!(!root.join(".gitignore").exists());
}

#[cfg(unix)]
#[test]
fn symlink_exclude_file_is_refused() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().join("repo");
    let escape = tmp.path().join("escape/exclude");
    fs::create_dir_all(&root).expect("repo");
    fs::create_dir_all(escape.parent().expect("escape parent")).expect("escape");
    fs::write(&escape, "outside\n").expect("escape file");
    fs::create_dir(root.join(".git")).expect("git dir");
    fs::create_dir(root.join(".git/info")).expect("info");
    std::os::unix::fs::symlink(&escape, root.join(".git/info/exclude")).expect("symlink exclude");
    let before = fs::read(&escape).expect("before");

    let error = ensure_effigy_ignored_in_git_root(&root).expect_err("symlink exclude");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(&escape).expect("after"), before);
    assert!(!root.join(".gitignore").exists());
}

#[test]
fn git_local_exclude_path_uses_common_dir_for_a_primary_checkout() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    fs::create_dir(root.join(".git")).expect("git dir");
    assert_eq!(git_local_exclude_path(root), root.join(".git/info/exclude"));
}

#[test]
fn dangling_gitfile_diagnostic_does_not_invent_exclude_path() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    fs::write(root.join(".git"), "gitdir: /nope/does/not/exist\n").expect("pointer");
    assert_eq!(git_local_exclude_path(root), root.join(".git"));
    assert!(
        !git_local_exclude_path(root)
            .to_string_lossy()
            .ends_with("info/exclude"),
        "must not invent .git/info/exclude through a gitfile"
    );
}

#[test]
fn pattern_listing_ignores_blank_lines() {
    assert!(pattern_already_listed(".effigy\n", ".effigy", &[".effigy"]));
    assert!(pattern_already_listed(
        ".effigy/\n",
        ".effigy",
        &[".effigy", ".effigy/"]
    ));
    assert!(!pattern_already_listed(
        "# .effigy\n",
        ".effigy",
        &[".effigy"]
    ));
}
