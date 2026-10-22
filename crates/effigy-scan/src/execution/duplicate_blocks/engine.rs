use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use super::*;

/// Multiplier for the rolling window hash over normalized line hashes.
const WINDOW_HASH_BASE: u64 = 0x100000001b3;

pub(crate) fn candidate_block_count(files: &[DuplicateBlockFile], seed_lines: usize) -> usize {
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

pub(crate) fn detect_duplicate_blocks(
    files: &[DuplicateBlockFile],
    options: &DuplicateBlockScanOptions,
) -> Result<Vec<DuplicateBlockFinding>, ScanError> {
    detect_duplicate_blocks_bounded(files, options, None)
}

pub(crate) fn detect_duplicate_blocks_bounded(
    files: &[DuplicateBlockFile],
    options: &DuplicateBlockScanOptions,
    deadline: Option<Instant>,
) -> Result<Vec<DuplicateBlockFinding>, ScanError> {
    let seed_lines = options.thresholds.warn;
    let mut seed_map = BTreeMap::<u64, Vec<DuplicateSeed>>::new();
    for (file_index, file) in files.iter().enumerate() {
        ensure_deadline(deadline)?;
        if file.lines.len() < seed_lines {
            continue;
        }
        for (start, fingerprint) in seed_fingerprints(&file.lines, seed_lines) {
            seed_map
                .entry(fingerprint)
                .or_default()
                .push(DuplicateSeed { file_index, start });
        }
    }

    let mut findings = Vec::<DuplicateBlockFinding>::new();
    let mut seen = BTreeSet::<String>::new();
    for seeds in seed_map.into_values() {
        ensure_deadline(deadline)?;
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
        if !seed_texts_match(files, &occurrences, seed_lines) {
            continue;
        }
        let Some(block) = extend_duplicate_block(files, &occurrences, seed_lines) else {
            continue;
        };
        let first = &block.occurrences[0];
        let first_file = &files[first.file_index];
        if is_literal_only_block(&first_file.lines[first.start..first.end]) {
            continue;
        }
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

/// Rolling fingerprints for every `seed_lines`-long window in a file. Each line
/// is hashed once, so a window costs one subtraction and one multiply instead of
/// re-hashing its whole text.
fn seed_fingerprints(lines: &[NormalizedCodeLine], seed_lines: usize) -> Vec<(usize, u64)> {
    let line_hashes = lines
        .iter()
        .map(|line| hash_line(&line.text))
        .collect::<Vec<u64>>();
    let mut fingerprints = Vec::with_capacity(lines.len().saturating_sub(seed_lines) + 1);
    if lines.len() < seed_lines {
        return fingerprints;
    }
    let mut leading_power = 1u64;
    for _ in 0..seed_lines.saturating_sub(1) {
        leading_power = leading_power.wrapping_mul(WINDOW_HASH_BASE);
    }
    let mut window = 0u64;
    for hash in line_hashes.iter().take(seed_lines) {
        window = window.wrapping_mul(WINDOW_HASH_BASE).wrapping_add(*hash);
    }
    fingerprints.push((0usize, window));
    for start in 1..=lines.len() - seed_lines {
        let outgoing = line_hashes[start - 1].wrapping_mul(leading_power);
        window = window.wrapping_sub(outgoing);
        window = window
            .wrapping_mul(WINDOW_HASH_BASE)
            .wrapping_add(line_hashes[start + seed_lines - 1]);
        fingerprints.push((start, window));
    }
    fingerprints
}

fn hash_line(text: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Hash windows can collide; only treat a seed group as a duplicate when the
/// normalized text really matches.
fn seed_texts_match(
    files: &[DuplicateBlockFile],
    occurrences: &[DuplicateSeed],
    seed_lines: usize,
) -> bool {
    let Some(reference_seed) = occurrences.first() else {
        return false;
    };
    let reference = &files[reference_seed.file_index].lines
        [reference_seed.start..reference_seed.start + seed_lines];
    occurrences.iter().skip(1).all(|seed| {
        files[seed.file_index].lines[seed.start..seed.start + seed_lines]
            .iter()
            .zip(reference.iter())
            .all(|(candidate, expected)| candidate.text == expected.text)
    })
}

fn ensure_deadline(deadline: Option<Instant>) -> Result<(), ScanError> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(ScanError::invocation(
            "doctor scan budget exhausted during duplicate evaluation",
        ));
    }
    Ok(())
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
    if start_offsets.contains(&0) {
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

pub(super) fn build_duplicate_block_finding(
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

pub(crate) struct DuplicateBlockFile {
    pub(crate) path: String,
    pub(crate) lines: Vec<NormalizedCodeLine>,
}

#[derive(Clone, Copy)]
struct DuplicateSeed {
    file_index: usize,
    start: usize,
}

pub(super) struct ExpandedDuplicateBlock {
    pub(super) occurrences: Vec<ExpandedDuplicateOccurrence>,
    pub(super) block_lines: usize,
}

pub(super) struct ExpandedDuplicateOccurrence {
    pub(super) file_index: usize,
    pub(super) start: usize,
    pub(super) end: usize,
}
