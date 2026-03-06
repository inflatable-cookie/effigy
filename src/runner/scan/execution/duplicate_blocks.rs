use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::super::model::{
    DuplicateBlockFinding, DuplicateBlockLocation, DuplicateBlockScanOptions,
    DuplicateBlockScanResult, DuplicateBlockSeverity,
};
use super::super::support::{
    classify_duplicate_block_severity, duplicate_block_severity_rank, is_generated_artifact,
    normalized_code_lines, rebase_finding_path, should_skip_path, trim_snippet, NormalizedCodeLine,
};
use super::{run_workspace_scan, walk_text_scan_files, RunnerError, ScanWorkspaceCounts};

pub(in crate::runner) fn run_duplicate_block_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &DuplicateBlockScanOptions,
) -> Result<DuplicateBlockScanResult, RunnerError> {
    run_workspace_scan(
        target_root,
        scan_roots,
        ScanWorkspaceCounts::default(),
        |root, skipped_roots| run_duplicate_block_scan_single(root, skipped_roots, options),
        |counts, result| {
            counts.scanned_files += result.scanned_files;
            counts.secondary += result.candidate_blocks;
        },
        |result| result.findings,
        |root, finding| {
            for location in &mut finding.locations {
                location.path = rebase_finding_path(target_root, root, &location.path);
            }
        },
        sort_duplicate_block_findings,
        |root, counts, findings| DuplicateBlockScanResult {
            root,
            scanned_files: counts.scanned_files,
            candidate_blocks: counts.secondary,
            findings,
            thresholds: options.thresholds.clone(),
        },
    )
}

fn run_duplicate_block_scan_single(
    root: &Path,
    skipped_roots: &[PathBuf],
    options: &DuplicateBlockScanOptions,
) -> Result<DuplicateBlockScanResult, RunnerError> {
    options.validate()?;
    let mut scanned_files = 0usize;
    let mut file_blocks = Vec::<DuplicateBlockFile>::new();

    walk_text_scan_files(
        root,
        skipped_roots,
        options.respect_gitignore,
        &options.include,
        &options.exclude,
        should_skip_path,
        |_path, rel, rel_str, contents| {
            if is_generated_artifact(rel, &contents) {
                return Ok(());
            }

            let lines = normalized_code_lines(rel, &contents);
            scanned_files += 1;
            if lines.len() >= options.thresholds.warn {
                file_blocks.push(DuplicateBlockFile {
                    path: rel_str.to_owned(),
                    lines,
                });
            }
            Ok(())
        },
    )?;

    let candidate_blocks = candidate_block_count(&file_blocks, options.thresholds.warn);
    let mut findings = detect_duplicate_blocks(&file_blocks, options)?;
    sort_duplicate_block_findings(&mut findings);

    Ok(DuplicateBlockScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_blocks,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

fn sort_duplicate_block_findings(findings: &mut [DuplicateBlockFinding]) {
    findings.sort_by(|left, right| {
        duplicate_block_severity_rank(right.severity)
            .cmp(&duplicate_block_severity_rank(left.severity))
            .then_with(|| right.block_lines.cmp(&left.block_lines))
            .then_with(|| right.occurrences.cmp(&left.occurrences))
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
}

fn candidate_block_count(files: &[DuplicateBlockFile], seed_lines: usize) -> usize {
    files
        .iter()
        .map(|file| {
            if file.lines.len() >= seed_lines {
                file.lines.len() - seed_lines + 1
            } else {
                0
            }
        })
        .sum()
}

fn detect_duplicate_blocks(
    files: &[DuplicateBlockFile],
    options: &DuplicateBlockScanOptions,
) -> Result<Vec<DuplicateBlockFinding>, RunnerError> {
    let seed_lines = options.thresholds.warn;
    let mut seed_map = BTreeMap::<String, Vec<DuplicateSeed>>::new();
    for (file_index, file) in files.iter().enumerate() {
        for start in 0..=file.lines.len().saturating_sub(seed_lines) {
            let fingerprint = fingerprint_normalized_lines(
                file.lines[start..start + seed_lines]
                    .iter()
                    .map(|line| line.text.as_str()),
            );
            seed_map
                .entry(fingerprint)
                .or_default()
                .push(DuplicateSeed { file_index, start });
        }
    }

    let mut findings = Vec::<DuplicateBlockFinding>::new();
    let mut seen = BTreeSet::<String>::new();
    for seeds in seed_map.into_values() {
        if seeds.len() < options.thresholds.min_occurrences {
            continue;
        }
        let distinct_files = seeds
            .iter()
            .map(|seed| seed.file_index)
            .collect::<BTreeSet<_>>();
        if distinct_files.len() < options.thresholds.min_occurrences {
            continue;
        }
        let occurrences = select_distinct_file_occurrences(seeds);
        if occurrences.len() < options.thresholds.min_occurrences {
            continue;
        }
        let Some(block) = extend_duplicate_block(files, &occurrences, seed_lines) else {
            continue;
        };
        let Some(severity) =
            classify_duplicate_block_severity(block.block_lines, &options.thresholds)
        else {
            continue;
        };
        let key = duplicate_block_key(files, &block);
        if !seen.insert(key) {
            continue;
        }
        findings.push(build_duplicate_block_finding(files, block, severity));
    }
    Ok(findings)
}

fn select_distinct_file_occurrences(mut seeds: Vec<DuplicateSeed>) -> Vec<DuplicateSeed> {
    seeds.sort_by(|left, right| {
        left.file_index
            .cmp(&right.file_index)
            .then_with(|| left.start.cmp(&right.start))
    });
    let mut chosen = Vec::new();
    let mut seen_files = BTreeSet::new();
    for seed in seeds {
        if seen_files.insert(seed.file_index) {
            chosen.push(seed);
        }
    }
    chosen
}

fn extend_duplicate_block(
    files: &[DuplicateBlockFile],
    occurrences: &[DuplicateSeed],
    seed_lines: usize,
) -> Option<ExpandedDuplicateBlock> {
    if occurrences.len() < 2 {
        return None;
    }
    let mut start_offsets = occurrences
        .iter()
        .map(|seed| seed.start)
        .collect::<Vec<_>>();
    let mut end_offsets = start_offsets
        .iter()
        .map(|start| start + seed_lines)
        .collect::<Vec<_>>();

    while can_extend_backward(files, occurrences, &start_offsets) {
        for start in &mut start_offsets {
            *start -= 1;
        }
    }
    while can_extend_forward(files, occurrences, &end_offsets) {
        for end in &mut end_offsets {
            *end += 1;
        }
    }

    let block_lines = end_offsets[0].saturating_sub(start_offsets[0]);
    (block_lines >= seed_lines).then_some(ExpandedDuplicateBlock {
        occurrences: occurrences
            .iter()
            .enumerate()
            .map(|(index, seed)| ExpandedDuplicateOccurrence {
                file_index: seed.file_index,
                start: start_offsets[index],
                end: end_offsets[index],
            })
            .collect(),
        block_lines,
    })
}

fn can_extend_backward(
    files: &[DuplicateBlockFile],
    occurrences: &[DuplicateSeed],
    start_offsets: &[usize],
) -> bool {
    if start_offsets.iter().any(|start| *start == 0) {
        return false;
    }
    let reference = &files[occurrences[0].file_index].lines[start_offsets[0] - 1].text;
    occurrences.iter().enumerate().skip(1).all(|(index, seed)| {
        files[seed.file_index].lines[start_offsets[index] - 1].text == *reference
    })
}

fn can_extend_forward(
    files: &[DuplicateBlockFile],
    occurrences: &[DuplicateSeed],
    end_offsets: &[usize],
) -> bool {
    if occurrences
        .iter()
        .enumerate()
        .any(|(index, seed)| end_offsets[index] >= files[seed.file_index].lines.len())
    {
        return false;
    }
    let reference = &files[occurrences[0].file_index].lines[end_offsets[0]].text;
    occurrences
        .iter()
        .enumerate()
        .skip(1)
        .all(|(index, seed)| files[seed.file_index].lines[end_offsets[index]].text == *reference)
}

fn duplicate_block_key(files: &[DuplicateBlockFile], block: &ExpandedDuplicateBlock) -> String {
    let mut key = String::new();
    key.push_str(&block.block_lines.to_string());
    for occurrence in &block.occurrences {
        let file = &files[occurrence.file_index];
        key.push('|');
        key.push_str(&file.path);
        key.push(':');
        key.push_str(&file.lines[occurrence.start].line_number.to_string());
        key.push('-');
        key.push_str(&file.lines[occurrence.end - 1].line_number.to_string());
    }
    key
}

fn build_duplicate_block_finding(
    files: &[DuplicateBlockFile],
    block: ExpandedDuplicateBlock,
    severity: DuplicateBlockSeverity,
) -> DuplicateBlockFinding {
    let first = &block.occurrences[0];
    let first_file = &files[first.file_index];
    let snippet_lines = first_file.lines[first.start..first.end]
        .iter()
        .take(4)
        .map(|line| line.text.as_str());
    let snippet = trim_snippet(&snippet_lines.collect::<Vec<_>>().join(" "), 120);
    let fingerprint = fingerprint_normalized_lines(
        first_file.lines[first.start..first.end]
            .iter()
            .map(|line| line.text.as_str()),
    );

    let mut locations = block
        .occurrences
        .into_iter()
        .map(|occurrence| {
            let file = &files[occurrence.file_index];
            DuplicateBlockLocation {
                path: file.path.clone(),
                start_line: file.lines[occurrence.start].line_number,
                end_line: file.lines[occurrence.end - 1].line_number,
            }
        })
        .collect::<Vec<_>>();
    locations.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.start_line.cmp(&right.start_line))
            .then_with(|| left.end_line.cmp(&right.end_line))
    });

    DuplicateBlockFinding {
        severity,
        block_lines: block.block_lines,
        occurrences: locations.len(),
        fingerprint,
        snippet,
        locations,
    }
}

fn fingerprint_normalized_lines<'a, I>(lines: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let mut hash = 0xcbf29ce484222325u64;
    for line in lines {
        for byte in line.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= u64::from(b'\n');
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

struct DuplicateBlockFile {
    path: String,
    lines: Vec<NormalizedCodeLine>,
}

#[derive(Clone, Copy)]
struct DuplicateSeed {
    file_index: usize,
    start: usize,
}

struct ExpandedDuplicateBlock {
    occurrences: Vec<ExpandedDuplicateOccurrence>,
    block_lines: usize,
}

struct ExpandedDuplicateOccurrence {
    file_index: usize,
    start: usize,
    end: usize,
}
