use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

use super::super::ScanError;
use super::paths::rel_path_str;

pub fn compile_glob_set(patterns: &[String], label: &str) -> Result<Option<GlobSet>, ScanError> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            ScanError::invocation(format!("invalid `{label}` glob `{pattern}`: {error}"))
        })?;
        builder.add(glob);
    }
    let set = builder.build().map_err(|error| {
        ScanError::invocation(format!("failed to compile `{label}` glob set: {error}"))
    })?;
    Ok(Some(set))
}

fn build_scan_walk(root: &Path, respect_gitignore: bool) -> WalkBuilder {
    let mut walk = WalkBuilder::new(root);
    let has_git_dir = root.join(".git").exists();
    walk.hidden(false)
        .ignore(respect_gitignore)
        .git_ignore(respect_gitignore)
        .git_exclude(respect_gitignore && has_git_dir)
        .git_global(respect_gitignore)
        .require_git(has_git_dir)
        .parents(respect_gitignore && has_git_dir)
        .follow_links(false);
    if respect_gitignore && !has_git_dir {
        for ignore_name in [".ignore", ".gitignore"] {
            let ignore_path = root.join(ignore_name);
            if ignore_path.is_file() {
                let _ = walk.add_ignore(ignore_path);
            }
        }
    }
    walk
}

pub fn walk_scan_files<ShouldSkip, Visit>(
    root: &Path,
    skipped_roots: &[PathBuf],
    respect_gitignore: bool,
    include_patterns: &[String],
    exclude_patterns: &[String],
    should_skip: ShouldSkip,
    mut visit: Visit,
) -> Result<(), ScanError>
where
    ShouldSkip: Fn(&Path, &str, Option<&GlobSet>, Option<&GlobSet>) -> bool,
    Visit: FnMut(&Path, &Path, &str) -> Result<(), ScanError>,
{
    let include = compile_glob_set(include_patterns, "include")?;
    let exclude = compile_glob_set(exclude_patterns, "exclude")?;
    let walk = build_scan_walk(root, respect_gitignore);

    for entry in walk.build() {
        let entry = entry.map_err(|error| {
            // A walk entry that cannot be produced (unreadable directory,
            // broken traversal) is a typed read failure naming the walk
            // root plus the underlying error, never a bare label.
            ScanError::read_failed(root.display(), error)
        })?;
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }

        let path = entry.path();
        if skipped_roots
            .iter()
            .any(|skip_root| path.starts_with(skip_root))
        {
            continue;
        }

        let rel = path.strip_prefix(root).unwrap_or(path);
        let rel_str = rel_path_str(rel)?;
        if rel_str.is_empty() || should_skip(rel, &rel_str, include.as_ref(), exclude.as_ref()) {
            continue;
        }

        visit(path, rel, &rel_str)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_walk_root_is_a_typed_read_failure() {
        // A walk root that cannot even be listed must fail closed as a
        // typed `ReadFailed` naming the root, never a bare label.
        let missing = Path::new("/definitely/not/here/effigy-scan-walk-test");
        let error = walk_scan_files(
            missing,
            &[],
            false,
            &[],
            &[],
            |_, _, _, _| false,
            |_, _, _| Ok(()),
        )
        .expect_err("missing walk root must fail");
        assert!(
            matches!(error, ScanError::ReadFailed { .. }),
            "got: {error}"
        );
        assert!(
            error.to_string().contains("effigy-scan-walk-test"),
            "got: {error}"
        );
    }
}
