//! The installed Effigy skill: the vendored snapshot `init` can write, and the
//! read-only status of what is already installed under the user's skill roots.

use std::path::{Path, PathBuf};

use super::check;
use super::contract::{INSTALLED_SKILL_ROOTS, INTERNAL_SKILL_METADATA_BLOCK, SKILL_FILES};
use super::{AgentCheck, AgentCheckStatus, AgentInitJob};
use crate::BuiltinError;

pub(super) fn ensure_skill_tree(
    root: &Path,
    job: AgentInitJob,
    apply: bool,
) -> Result<AgentCheck, BuiltinError> {
    let base = ".agents/skills/effigy";
    let mut missing = 0usize;
    let mut stale = 0usize;
    let mut changed = 0usize;

    for (relative, contents) in SKILL_FILES {
        let desired = vendored_skill_contents(relative, contents);
        let path = root.join(base).join(relative);
        match std::fs::read_to_string(&path) {
            Ok(existing) if existing == desired => {}
            Ok(_) if apply => {
                write_file(&path, &desired)?;
                changed += 1;
            }
            Ok(_) => stale += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && apply => {
                write_file(&path, &desired)?;
                missing += 1;
                changed += 1;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing += 1,
            Err(error) => return Err(BuiltinError::task_invocation_failed_read(&path, error)),
        }
    }

    let detail = Some(format!(
        "{} file(s), {missing} missing, {stale} stale, {changed} changed",
        SKILL_FILES.len()
    ));
    let status = if changed > 0 {
        if missing > 0 {
            AgentCheckStatus::Created
        } else {
            AgentCheckStatus::Updated
        }
    } else if missing > 0 {
        AgentCheckStatus::WouldCreate
    } else if stale > 0 {
        AgentCheckStatus::WouldUpdate
    } else {
        AgentCheckStatus::Present
    };
    Ok(check(job, base, status, "sync_skill_tree", detail))
}

pub(super) fn inspect_skill_guidance(
    root: &Path,
    job: AgentInitJob,
) -> Result<AgentCheck, BuiltinError> {
    let local_skill = root.join(".agents/skills/effigy");
    let local_status = if std::fs::symlink_metadata(&local_skill).is_ok() {
        format!(
            "existing project-local skill at {} will be preserved",
            local_skill.display()
        )
    } else {
        "no project-local skill is required".to_owned()
    };
    let install_status = installed_skill_status(home_dir().as_deref());
    Ok(check(
        job,
        ".agents/skills/effigy",
        AgentCheckStatus::Present,
        "inspect_skill_guidance",
        Some(format!("{local_status}; {}", install_status.description())),
    ))
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum InstalledSkillStatus {
    Found {
        canonical_root: PathBuf,
        aliases: Vec<PathBuf>,
    },
    Missing,
    Ambiguous(Vec<PathBuf>),
    Unverifiable(Vec<PathBuf>),
    HomeUnavailable,
}

impl InstalledSkillStatus {
    pub(super) fn description(&self) -> String {
        match self {
            Self::Found {
                canonical_root,
                aliases,
            } if aliases.len() > 1 => format!(
                "installed Effigy guidance resolves to canonical root {} through {}",
                canonical_root.display(),
                display_paths(aliases)
            ),
            Self::Found { canonical_root, .. } => format!(
                "installed Effigy guidance found at canonical root {}",
                canonical_root.display()
            ),
            Self::Missing => format!(
                "no installed Effigy guidance found under the supported user roots ({}); install with `npx skills add inflatable-cookie/effigy -g`, then start a fresh agent context; init does not install or copy a skill",
                INSTALLED_SKILL_ROOTS.join(", ")
            ),
            Self::Ambiguous(roots) => format!(
                "multiple distinct installed Effigy guidance roots were found ({}); choose one explicitly; Effigy named task lookup fails closed on distinct global roots",
                display_paths(roots)
            ),
            Self::Unverifiable(roots) => format!(
                "could not canonicalize installed Effigy guidance at {}; resolve these paths before choosing a source",
                display_paths(roots)
            ),
            Self::HomeUnavailable => format!(
                "home directory is unavailable, so installed Effigy guidance could not be checked under {}; install with `npx skills add inflatable-cookie/effigy -g` and verify from a fresh agent context",
                INSTALLED_SKILL_ROOTS.join(", ")
            ),
        }
    }
}

pub(super) fn installed_skill_status(home: Option<&Path>) -> InstalledSkillStatus {
    let Some(home) = home else {
        return InstalledSkillStatus::HomeUnavailable;
    };

    let mut canonical_roots = std::collections::BTreeMap::<PathBuf, Vec<PathBuf>>::new();
    let mut unverifiable = Vec::new();
    for root in INSTALLED_SKILL_ROOTS {
        let candidate = home.join(root).join("effigy");
        match std::fs::metadata(candidate.join("SKILL.md")) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                unverifiable.push(candidate);
                continue;
            }
        }
        match std::fs::canonicalize(&candidate) {
            Ok(canonical) => canonical_roots
                .entry(canonical)
                .or_default()
                .push(candidate),
            Err(_) => unverifiable.push(candidate),
        }
    }

    if !unverifiable.is_empty() {
        return InstalledSkillStatus::Unverifiable(unverifiable);
    }
    match canonical_roots.len() {
        0 => InstalledSkillStatus::Missing,
        1 => {
            let (canonical_root, aliases) = canonical_roots
                .into_iter()
                .next()
                .expect("one canonical root was counted");
            InstalledSkillStatus::Found {
                canonical_root,
                aliases,
            }
        }
        _ => InstalledSkillStatus::Ambiguous(canonical_roots.into_keys().collect()),
    }
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
}

fn vendored_skill_contents(relative: &str, contents: &str) -> String {
    if relative != "SKILL.md" {
        return contents.to_owned();
    }
    inject_internal_skill_metadata(contents)
}

/// The vendored copy is internal: it is Effigy's own bundled skill, not a
/// public catalogue entry, so its frontmatter says so once.
fn inject_internal_skill_metadata(contents: &str) -> String {
    if contents.contains("internal: true") {
        return contents.to_owned();
    }
    let Some(rest) = contents.strip_prefix("---\n") else {
        return contents.to_owned();
    };
    let Some(frontmatter_end) = rest.find("\n---\n") else {
        return contents.to_owned();
    };
    let insert_at = 4 + frontmatter_end + 1;
    let mut next = String::with_capacity(contents.len() + INTERNAL_SKILL_METADATA_BLOCK.len());
    next.push_str(&contents[..insert_at]);
    next.push_str(INTERNAL_SKILL_METADATA_BLOCK);
    next.push_str(&contents[insert_at..]);
    next
}

fn write_file(path: &Path, contents: &str) -> Result<(), BuiltinError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| BuiltinError::task_invocation_failed_write(parent, error))?;
        }
    }
    std::fs::write(path, contents.as_bytes())
        .map_err(|error| BuiltinError::task_invocation_failed_write(path, error))
}

#[cfg(test)]
mod tests {
    use super::super::contract::SKILL_FILES;
    use super::*;

    fn collect_relative_files(root: &Path, current: &Path, files: &mut Vec<String>) {
        for entry in std::fs::read_dir(current).expect("skill directory should be readable") {
            let entry = entry.expect("skill directory entry should be readable");
            let path = entry.path();
            if path.is_dir() {
                collect_relative_files(root, &path, files);
            } else if path.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .expect("skill file should be inside skill root");
                files.push(
                    relative
                        .components()
                        .map(|component| component.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }

    #[test]
    fn managed_skill_registry_matches_public_skill_tree() {
        let skill_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/effigy");
        let mut public_files = Vec::new();
        collect_relative_files(&skill_root, &skill_root, &mut public_files);
        public_files.sort();

        let mut managed_files = SKILL_FILES
            .iter()
            .map(|(relative, _)| (*relative).to_owned())
            .collect::<Vec<_>>();
        managed_files.sort();

        assert_eq!(managed_files, public_files);
    }

    #[test]
    fn inject_internal_skill_metadata_adds_internal_flag_inside_frontmatter() {
        let input = "---\nname: effigy\ndescription: demo\n---\n\n# Skill\n";
        let output = inject_internal_skill_metadata(input);
        assert!(output.starts_with("---\nname: effigy\ndescription: demo\n"));
        assert!(output.contains("\nmetadata:\n  internal: true\n"));
        assert!(output.contains("\n---\n\n# Skill\n"));
        assert!(output.ends_with("\n# Skill\n"));
    }

    #[test]
    fn inject_internal_skill_metadata_is_idempotent() {
        let input =
            "---\nname: effigy\ndescription: demo\nmetadata:\n  internal: true\n---\n\n# Skill\n";
        let output = inject_internal_skill_metadata(input);
        assert_eq!(output, input);
    }

    #[test]
    fn init_installed_skill_status_collapses_symlink_aliases_to_one_canonical_root() {
        let home = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-init-skill-aliases-{}",
            std::process::id()
        ))
        .expect("fixture directory");
        let _ = std::fs::remove_dir_all(&home);
        let canonical = home.join(".agents/skills/effigy");
        std::fs::create_dir_all(&canonical).expect("canonical skill root");
        std::fs::write(canonical.join("SKILL.md"), "---\nname: effigy\n---\n")
            .expect("skill marker");

        #[cfg(unix)]
        {
            let alias = home.join(".claude/skills/effigy");
            std::fs::create_dir_all(alias.parent().expect("alias parent"))
                .expect("alias parent directory");
            std::os::unix::fs::symlink(&canonical, &alias).expect("skill alias");

            let status = installed_skill_status(Some(&home));
            let InstalledSkillStatus::Found {
                canonical_root,
                aliases,
            } = status
            else {
                panic!("expected one canonical install, got {status:?}");
            };
            assert_eq!(canonical_root, std::fs::canonicalize(&canonical).unwrap());
            assert_eq!(aliases.len(), 2);
        }

        #[cfg(not(unix))]
        {
            let status = installed_skill_status(Some(&home));
            assert!(matches!(status, InstalledSkillStatus::Found { .. }));
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn init_installed_skill_status_reports_distinct_global_roots_as_ambiguous() {
        let home = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-init-skill-duplicates-{}",
            std::process::id()
        ))
        .expect("fixture directory");
        let _ = std::fs::remove_dir_all(&home);
        for root in [".agents/skills/effigy", ".cursor/skills/effigy"] {
            let skill = home.join(root);
            std::fs::create_dir_all(&skill).expect("skill root");
            std::fs::write(skill.join("SKILL.md"), "---\nname: effigy\n---\n")
                .expect("skill marker");
        }

        let status = installed_skill_status(Some(&home));
        assert!(matches!(status, InstalledSkillStatus::Ambiguous(ref roots) if roots.len() == 2));
        assert!(status.description().contains("choose one explicitly"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn init_installed_skill_status_reports_missing_install_without_claiming_success() {
        let home = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-init-skill-missing-{}",
            std::process::id()
        ))
        .expect("fixture directory");
        let _ = std::fs::remove_dir_all(&home);

        let status = installed_skill_status(Some(&home));
        assert_eq!(status, InstalledSkillStatus::Missing);
        assert!(status
            .description()
            .contains("no installed Effigy guidance"));
        assert!(status.description().contains("npx skills add"));
        assert!(status.description().contains("does not install or copy"));
    }
}
