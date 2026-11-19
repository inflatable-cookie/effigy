use std::path::{Path, PathBuf};
use std::time::Instant;

use globset::GlobSet;

use super::duplicate_blocks::{
    candidate_block_count, detect_duplicate_blocks_bounded, DuplicateBlockFile,
};
use super::ScanError;
use crate::model::*;
use crate::support::*;

#[derive(Debug, Clone)]
pub struct DoctorScanInventoryOptions {
    pub god_files: GodFileScanOptions,
    pub duplicate_blocks: DuplicateBlockScanOptions,
}

#[derive(Debug, Clone)]
pub struct DoctorScanInventoryResult {
    pub physical_walks: usize,
    pub god_files: Option<GodFileScanResult>,
    pub duplicate_blocks: Option<DuplicateBlockScanResult>,
}

#[derive(Debug, Clone)]
struct Observation {
    rel_str: String,
    generated: bool,
    code_lines: usize,
    total_lines: usize,
    god_file_eligible: bool,
    duplicate_eligible: bool,
    normalized_lines: Vec<NormalizedCodeLine>,
}

struct PreparedOptions {
    god_files: Filter,
    duplicate_blocks: Filter,
}

struct Filter {
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
}

impl Filter {
    fn new(include: &[String], exclude: &[String]) -> Result<Self, ScanError> {
        Ok(Self {
            include: compile_glob_set(include, "include")?,
            exclude: compile_glob_set(exclude, "exclude")?,
        })
    }

    fn skips_code(&self, observation: &Observation) -> bool {
        should_skip_path(
            Path::new(&observation.rel_str),
            &observation.rel_str,
            self.include.as_ref(),
            self.exclude.as_ref(),
        )
    }
}

pub fn run_doctor_scan_inventory(
    scope_root: &Path,
    pruned_roots: &[PathBuf],
    options: &DoctorScanInventoryOptions,
    deadline: Option<Instant>,
) -> Result<DoctorScanInventoryResult, ScanError> {
    validate_options(options)?;
    let prepared = prepare_options(options)?;
    let respect_gitignore = enabled_gitignore_postures(options).all(|value| value);
    let mut observations = Vec::new();
    walk_scan_files(
        scope_root,
        pruned_roots,
        respect_gitignore,
        &[],
        &[],
        should_skip_inventory_path,
        |path, rel, rel_str| {
            ensure_budget(deadline)?;
            // Byte-walk read: unreadable files are a typed read failure.
            // Non-UTF-8 bytes are not a refusal here; `analyze_file`
            // observes them as binary below.
            let bytes = std::fs::read(path)
                .map_err(|error| ScanError::read_failed(path.display(), error))?;
            observations.push(analyze_file(rel, rel_str, &bytes, &prepared));
            Ok(())
        },
    )?;
    ensure_budget(deadline)?;

    let god_files = options
        .god_files
        .doctor_enabled
        .then(|| evaluate_god_files(scope_root, &observations, &options.god_files, deadline))
        .transpose()?;
    let duplicate_blocks = options
        .duplicate_blocks
        .doctor_enabled
        .then(|| {
            evaluate_duplicate_blocks(
                scope_root,
                &observations,
                &options.duplicate_blocks,
                deadline,
            )
        })
        .transpose()?;
    ensure_budget(deadline)?;

    Ok(DoctorScanInventoryResult {
        physical_walks: 1,
        god_files,
        duplicate_blocks,
    })
}

fn prepare_options(options: &DoctorScanInventoryOptions) -> Result<PreparedOptions, ScanError> {
    Ok(PreparedOptions {
        god_files: Filter::new(&options.god_files.include, &options.god_files.exclude)?,
        duplicate_blocks: Filter::new(
            &options.duplicate_blocks.include,
            &options.duplicate_blocks.exclude,
        )?,
    })
}

fn analyze_file(rel: &Path, rel_str: &str, bytes: &[u8], options: &PreparedOptions) -> Observation {
    // Non-UTF-8 files are binary for the code scanners: they carry no
    // code lines and never match generated-content markers, instead of
    // silently scanning replacement characters. `walk_text_scan_files`
    // (the text-scan path) still reports unreadable/non-UTF-8 files as a
    // typed `ScanError`; this loss-free branch only applies to the doctor
    // inventory's byte walk, which must observe every file.
    let Ok(contents) = std::str::from_utf8(bytes) else {
        let stub = Observation {
            rel_str: rel_str.to_owned(),
            generated: false,
            code_lines: 0,
            total_lines: 0,
            god_file_eligible: false,
            duplicate_eligible: false,
            normalized_lines: Vec::new(),
        };
        return Observation {
            god_file_eligible: !options.god_files.skips_code(&stub),
            duplicate_eligible: !options.duplicate_blocks.skips_code(&stub),
            ..stub
        };
    };
    let generated = is_generated_artifact(rel, contents);
    let lines = normalized_code_lines(rel, contents);
    let stub = Observation {
        rel_str: rel_str.to_owned(),
        generated,
        code_lines: lines.len(),
        total_lines: contents.lines().count(),
        god_file_eligible: false,
        duplicate_eligible: false,
        normalized_lines: lines,
    };
    Observation {
        god_file_eligible: !options.god_files.skips_code(&stub),
        duplicate_eligible: !options.duplicate_blocks.skips_code(&stub),
        ..stub
    }
}

fn validate_options(options: &DoctorScanInventoryOptions) -> Result<(), ScanError> {
    if options.god_files.doctor_enabled {
        options.god_files.validate()?;
    }
    if options.duplicate_blocks.doctor_enabled {
        options.duplicate_blocks.validate()?;
    }
    Ok(())
}

fn enabled_gitignore_postures(
    options: &DoctorScanInventoryOptions,
) -> impl Iterator<Item = bool> + '_ {
    [
        (
            options.god_files.doctor_enabled,
            options.god_files.respect_gitignore,
        ),
        (
            options.duplicate_blocks.doctor_enabled,
            options.duplicate_blocks.respect_gitignore,
        ),
    ]
    .into_iter()
    .filter_map(|(enabled, posture)| enabled.then_some(posture))
}

fn ensure_budget(deadline: Option<Instant>) -> Result<(), ScanError> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(ScanError::budget_exhausted("inventory"));
    }
    Ok(())
}

fn evaluate_god_files(
    root: &Path,
    observations: &[Observation],
    options: &GodFileScanOptions,
    deadline: Option<Instant>,
) -> Result<GodFileScanResult, ScanError> {
    let mut findings = Vec::new();
    let mut scanned_files = 0;
    let mut skipped_generated = 0;
    for observation in observations {
        ensure_budget(deadline)?;
        if !observation.god_file_eligible {
            continue;
        }
        if observation.generated {
            skipped_generated += 1;
            continue;
        }
        scanned_files += 1;
        let code_lines = observation.code_lines;
        let path = Path::new(&observation.rel_str);
        let thresholds = options.thresholds_for(path);
        if let Some(severity) = classify_severity(code_lines, &thresholds) {
            findings.push(GodFileFinding {
                path: observation.rel_str.clone(),
                code_lines,
                total_lines: observation.total_lines,
                severity,
                language: god_file_language(path),
            });
        }
    }
    findings.sort_by(|left, right| {
        severity_rank(right.severity)
            .cmp(&severity_rank(left.severity))
            .then_with(|| right.code_lines.cmp(&left.code_lines))
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(GodFileScanResult {
        root: root.display().to_string(),
        scanned_files,
        skipped_generated,
        findings,
        thresholds: options.thresholds,
        thresholds_source: options.thresholds_source(),
    })
}

fn evaluate_duplicate_blocks(
    root: &Path,
    observations: &[Observation],
    options: &DuplicateBlockScanOptions,
    deadline: Option<Instant>,
) -> Result<DuplicateBlockScanResult, ScanError> {
    let mut scanned_files = 0;
    let mut files = Vec::new();
    for observation in observations {
        ensure_budget(deadline)?;
        if !observation.duplicate_eligible || observation.generated {
            continue;
        }
        scanned_files += 1;
        let lines = observation.normalized_lines.clone();
        if lines.len() >= options.thresholds.warn {
            files.push(DuplicateBlockFile {
                path: observation.rel_str.clone(),
                lines,
            });
        }
    }
    let candidate_blocks = candidate_block_count(&files, options.thresholds.warn);
    let mut findings = detect_duplicate_blocks_bounded(&files, options, deadline)?;
    findings.sort_by(|left, right| {
        duplicate_block_severity_rank(right.severity)
            .cmp(&duplicate_block_severity_rank(left.severity))
            .then_with(|| right.block_lines.cmp(&left.block_lines))
            .then_with(|| right.occurrences.cmp(&left.occurrences))
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
    Ok(DuplicateBlockScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_blocks,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{run_duplicate_block_scan_workspace, run_god_file_scan_workspace};

    fn options() -> DoctorScanInventoryOptions {
        DoctorScanInventoryOptions {
            god_files: GodFileScanOptions::default(),
            duplicate_blocks: DuplicateBlockScanOptions {
                doctor_enabled: true,
                ..DuplicateBlockScanOptions::default()
            },
        }
    }

    #[test]
    fn shared_inventory_matches_enabled_standalone_scans_with_one_walk() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::write(
            root.join("src/lib.rs"),
            "// TODO: split this module\nfn alpha() { println!(\"alpha\"); }\n",
        )
        .expect("source");
        std::fs::write(
            root.join("src/generated.rs"),
            "// generated file\nfn x() {}\n",
        )
        .expect("generated");
        let options = options();

        let shared =
            run_doctor_scan_inventory(root, &[], &options, None).expect("shared inventory");

        assert_eq!(shared.physical_walks, 1);
        assert_eq!(
            shared.god_files,
            Some(
                run_god_file_scan_workspace(root, &[root.to_path_buf()], &options.god_files)
                    .expect("god files")
            )
        );
        assert_eq!(
            shared.duplicate_blocks,
            Some(
                run_duplicate_block_scan_workspace(
                    root,
                    &[root.to_path_buf()],
                    &options.duplicate_blocks,
                )
                .expect("duplicate blocks")
            )
        );
    }

    #[test]
    fn pruned_sibling_is_not_observed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let sibling = root.join("apps/sibling");
        std::fs::create_dir_all(&sibling).expect("sibling");
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("root source");
        std::fs::write(sibling.join("secret.rs"), "fn sibling() {}\n").expect("sibling source");
        let options = options();

        let result =
            run_doctor_scan_inventory(root, std::slice::from_ref(&sibling), &options, None)
                .expect("root inventory");
        assert_eq!(result.god_files.expect("god files").scanned_files, 1);
    }

    #[test]
    fn expired_deadline_stops_the_inventory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("source");
        let options = options();
        let error = run_doctor_scan_inventory(root, &[], &options, Some(Instant::now()))
            .expect_err("deadline should fail");
        assert!(error.is_budget_exhausted(), "{error}");
        assert!(error.to_string().contains("budget exhausted"));
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_inventory_file_is_a_typed_read_failure() {
        // The byte walk lists the permissionless file, so its read fails
        // with `PermissionDenied`: typed `ReadFailed`, not a generic string.
        // Root can read anything, so only assert when the read refuses.
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let path = root.join("secret.rs");
        std::fs::write(&path, b"fn secret() {}\n").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        if std::fs::read(&path).is_err() {
            let options = options();
            let error = run_doctor_scan_inventory(root, &[], &options, None)
                .expect_err("unreadable inventory file must fail");
            assert!(
                matches!(error, ScanError::ReadFailed { .. }),
                "got: {error}"
            );
            assert!(error.to_string().contains("secret.rs"), "got: {error}");
        }
    }

    #[test]
    fn non_utf8_file_is_binary_not_lossy_text() {
        // Non-UTF-8 bytes must not be scanned as replacement characters:
        // the observation carries no code lines and no generated flag from
        // content markers.
        let rel = std::path::Path::new("src/binary.rs");
        let bytes = vec![0xff, 0xfe, 0x62, 0x79, 0x74, 0x65, 0x73];
        let prepared = PreparedOptions {
            god_files: Filter::new(&[], &[]).expect("filter"),
            duplicate_blocks: Filter::new(&[], &[]).expect("filter"),
        };
        let observation = analyze_file(rel, "src/binary.rs", &bytes, &prepared);
        assert_eq!(observation.code_lines, 0);
        assert_eq!(observation.total_lines, 0);
        assert!(!observation.generated);
        assert!(observation.normalized_lines.is_empty());
    }
}
