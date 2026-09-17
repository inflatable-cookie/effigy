use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use globset::GlobSet;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    pub comment_ratio: CommentRatioScanOptions,
    pub generated_assets: GeneratedAssetScanOptions,
    pub generated_in_src: GeneratedInSrcScanOptions,
    pub attention_markers: AttentionMarkerScanOptions,
    pub stale_suppressions: StaleSuppressionScanOptions,
}

#[derive(Debug, Clone)]
pub struct DoctorScanInventoryResult {
    pub physical_walks: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub invalid_cache_entries: usize,
    pub warnings: Vec<String>,
    pub god_files: Option<GodFileScanResult>,
    pub duplicate_blocks: Option<DuplicateBlockScanResult>,
    pub comment_ratio: Option<CommentRatioScanResult>,
    pub generated_assets: Option<GeneratedAssetScanResult>,
    pub generated_in_src: Option<GeneratedInSrcScanResult>,
    pub attention_markers: Option<AttentionMarkerScanResult>,
    pub stale_suppressions: Option<StaleSuppressionScanResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Observation {
    rel_str: String,
    identity: String,
    bytes: usize,
    generated: bool,
    code_lines: usize,
    total_lines: usize,
    comment_lines: usize,
    god_file_eligible: bool,
    duplicate_eligible: bool,
    comment_ratio_eligible: bool,
    generated_asset_eligible: bool,
    generated_in_src_eligible: bool,
    attention_eligible: bool,
    stale_eligible: bool,
    generated_asset_reason: Option<String>,
    generated_in_src: Option<CachedGeneratedInSrc>,
    normalized_lines: Vec<CachedNormalizedLine>,
    attention_hits: Vec<CachedMarkerHit>,
    stale_hits: Vec<CachedMarkerHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedGeneratedInSrc {
    category: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedNormalizedLine {
    line_number: usize,
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedMarkerHit {
    line: usize,
    category: String,
    severity: String,
    marker: String,
    snippet: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheGeneration {
    schema: String,
    config_identity: String,
    files: BTreeMap<String, Observation>,
}

struct PreparedOptions {
    god_files: Filter,
    duplicate_blocks: Filter,
    comment_ratio: Filter,
    generated_assets: Filter,
    generated_in_src: Filter,
    generated_source_roots: Option<GlobSet>,
    attention_markers: Filter,
    stale_suppressions: Filter,
    attention_patterns: Vec<(AttentionMarkerSeverity, String, String)>,
    stale_patterns: Vec<(StaleSuppressionSeverity, String, String)>,
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

    fn skips_asset(&self, observation: &Observation) -> bool {
        should_skip_generated_asset_path(
            Path::new(&observation.rel_str),
            &observation.rel_str,
            self.include.as_ref(),
            self.exclude.as_ref(),
        )
    }
}

pub fn run_doctor_scan_inventory(
    workspace_root: &Path,
    scope_alias: &str,
    scope_root: &Path,
    pruned_roots: &[PathBuf],
    options: &DoctorScanInventoryOptions,
    refresh: bool,
    deadline: Option<Instant>,
) -> Result<DoctorScanInventoryResult, ScanError> {
    validate_options(options)?;
    let prepared = prepare_options(options)?;
    let respect_gitignore = enabled_gitignore_postures(options).all(|value| value);
    let config_identity = config_identity(scope_root, pruned_roots, options);
    let cache_paths = cache_paths(workspace_root, scope_alias, scope_root);
    let (cached_files, mut invalid_cache_entries, mut warnings) =
        load_cache(&cache_paths.current, &config_identity, refresh);
    let git_identities = git_identities(scope_root);
    let mut observations = Vec::new();
    let mut cache_hits = 0usize;
    let mut cache_misses = 0usize;
    walk_scan_files(
        scope_root,
        pruned_roots,
        respect_gitignore,
        &[],
        &[],
        should_skip_generated_asset_path,
        |path, rel, rel_str| {
            ensure_budget(deadline)?;
            let clean_git_identity = git_identities.get(rel_str).cloned();
            if let Some(identity) = clean_git_identity.as_deref() {
                if let Some(cached) = cached_files
                    .get(rel_str)
                    .filter(|cached| cached.identity == identity)
                {
                    cache_hits += 1;
                    observations.push(cached.clone());
                    return Ok(());
                }
            }
            let bytes = std::fs::read(path).map_err(|error| {
                ScanError::invocation(format!(
                    "doctor inventory read failed for {}: {error}",
                    path.display()
                ))
            })?;
            let identity = clean_git_identity.unwrap_or_else(|| content_identity(&bytes));
            if let Some(cached) = cached_files
                .get(rel_str)
                .filter(|cached| cached.identity == identity)
            {
                cache_hits += 1;
                observations.push(cached.clone());
                return Ok(());
            }
            cache_misses += 1;
            observations.push(analyze_file(rel, rel_str, &bytes, identity, &prepared));
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
    let comment_ratio = options
        .comment_ratio
        .doctor_enabled
        .then(|| {
            evaluate_comment_ratio(scope_root, &observations, &options.comment_ratio, deadline)
        })
        .transpose()?;
    let generated_assets = options
        .generated_assets
        .doctor_enabled
        .then(|| {
            evaluate_generated_assets(
                scope_root,
                &observations,
                &options.generated_assets,
                deadline,
            )
        })
        .transpose()?;
    let generated_in_src = options
        .generated_in_src
        .doctor_enabled
        .then(|| {
            evaluate_generated_in_src(
                scope_root,
                &observations,
                &options.generated_in_src,
                deadline,
            )
        })
        .transpose()?;
    let attention_markers = options
        .attention_markers
        .doctor_enabled
        .then(|| {
            evaluate_attention_markers(
                scope_root,
                &observations,
                &options.attention_markers,
                deadline,
            )
        })
        .transpose()?;
    let stale_suppressions = options
        .stale_suppressions
        .doctor_enabled
        .then(|| {
            evaluate_stale_suppressions(
                scope_root,
                &observations,
                &options.stale_suppressions,
                deadline,
            )
        })
        .transpose()?;
    ensure_budget(deadline)?;

    let files = observations
        .iter()
        .cloned()
        .map(|observation| (observation.rel_str.clone(), observation))
        .collect::<BTreeMap<_, _>>();
    if let Err(error) = publish_cache(
        &cache_paths,
        &CacheGeneration {
            schema: "effigy.doctor.cache.v1".to_owned(),
            config_identity: config_identity.clone(),
            files,
        },
    ) {
        invalid_cache_entries += 1;
        warnings.push(error);
    }

    Ok(DoctorScanInventoryResult {
        physical_walks: 1,
        cache_hits,
        cache_misses,
        invalid_cache_entries,
        warnings,
        god_files,
        duplicate_blocks,
        comment_ratio,
        generated_assets,
        generated_in_src,
        attention_markers,
        stale_suppressions,
    })
}

fn prepare_options(options: &DoctorScanInventoryOptions) -> Result<PreparedOptions, ScanError> {
    Ok(PreparedOptions {
        god_files: Filter::new(&options.god_files.include, &options.god_files.exclude)?,
        duplicate_blocks: Filter::new(
            &options.duplicate_blocks.include,
            &options.duplicate_blocks.exclude,
        )?,
        comment_ratio: Filter::new(
            &options.comment_ratio.include,
            &options.comment_ratio.exclude,
        )?,
        generated_assets: Filter::new(
            &options.generated_assets.include,
            &options.generated_assets.exclude,
        )?,
        generated_in_src: Filter::new(
            &options.generated_in_src.include,
            &options.generated_in_src.exclude,
        )?,
        generated_source_roots: compile_glob_set(
            &options.generated_in_src.source_roots,
            "source_root",
        )?,
        attention_markers: Filter::new(
            &options.attention_markers.include,
            &options.attention_markers.exclude,
        )?,
        stale_suppressions: Filter::new(
            &options.stale_suppressions.include,
            &options.stale_suppressions.exclude,
        )?,
        attention_patterns: compile_attention_marker_patterns(&options.attention_markers.patterns),
        stale_patterns: compile_stale_suppression_patterns(&options.stale_suppressions.patterns),
    })
}

fn analyze_file(
    rel: &Path,
    rel_str: &str,
    bytes: &[u8],
    identity: String,
    options: &PreparedOptions,
) -> Observation {
    let contents = String::from_utf8_lossy(bytes);
    let generated = is_generated_artifact(rel, &contents);
    let lines = normalized_code_lines(rel, &contents);
    let ratios = comment_ratio_counts(rel, &contents);
    let sample = String::from_utf8_lossy(&bytes[..bytes.len().min(16 * 1024)]).to_ascii_lowercase();
    let stub = Observation {
        rel_str: rel_str.to_owned(),
        identity,
        bytes: bytes.len(),
        generated,
        code_lines: lines.len(),
        total_lines: contents.lines().count(),
        comment_lines: ratios.comment_lines,
        god_file_eligible: false,
        duplicate_eligible: false,
        comment_ratio_eligible: false,
        generated_asset_eligible: false,
        generated_in_src_eligible: false,
        attention_eligible: false,
        stale_eligible: false,
        generated_asset_reason: generated_asset_reason(rel, &sample),
        generated_in_src: generated_in_src_reason(rel, &sample).map(|(category, reason)| {
            CachedGeneratedInSrc {
                category: category.as_str().to_owned(),
                reason,
            }
        }),
        normalized_lines: lines
            .into_iter()
            .map(|line| CachedNormalizedLine {
                line_number: line.line_number,
                text: line.text,
            })
            .collect(),
        attention_hits: marker_hits_attention(&contents, &options.attention_patterns),
        stale_hits: marker_hits_stale(&contents, &options.stale_patterns),
    };
    Observation {
        god_file_eligible: !options.god_files.skips_code(&stub),
        duplicate_eligible: !options.duplicate_blocks.skips_code(&stub),
        comment_ratio_eligible: !options.comment_ratio.skips_code(&stub),
        generated_asset_eligible: !options.generated_assets.skips_asset(&stub),
        generated_in_src_eligible: !options.generated_in_src.skips_asset(&stub)
            && options
                .generated_source_roots
                .as_ref()
                .is_some_and(|set| set.is_match(rel_str)),
        attention_eligible: !options.attention_markers.skips_code(&stub),
        stale_eligible: !options.stale_suppressions.skips_code(&stub),
        ..stub
    }
}

fn marker_hits_attention(
    contents: &str,
    patterns: &[(AttentionMarkerSeverity, String, String)],
) -> Vec<CachedMarkerHit> {
    let mut hits = Vec::new();
    for (line_number, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let mut matched_keys = BTreeSet::new();
        for (severity, marker, marker_lower) in patterns {
            if !attention_marker_matches_line(raw_line, marker_lower)
                || !matched_keys.insert(marker_lower.trim_start_matches('@').to_owned())
            {
                continue;
            }
            hits.push(CachedMarkerHit {
                line: line_number + 1,
                category: attention_marker_category(marker_lower).as_str().to_owned(),
                severity: severity.as_str().to_owned(),
                marker: marker.clone(),
                snippet: trim_snippet(line, 120),
            });
        }
    }
    hits
}

fn marker_hits_stale(
    contents: &str,
    patterns: &[(StaleSuppressionSeverity, String, String)],
) -> Vec<CachedMarkerHit> {
    let mut hits = Vec::new();
    for (line_number, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let mut matched_keys = BTreeSet::new();
        for (severity, marker, marker_lower) in patterns {
            let key = if marker_lower.starts_with("#[allow(") {
                "#[allow(".to_owned()
            } else {
                marker_lower.clone()
            };
            if !stale_suppression_matches_line(raw_line, marker_lower) || !matched_keys.insert(key)
            {
                continue;
            }
            hits.push(CachedMarkerHit {
                line: line_number + 1,
                category: stale_suppression_category(marker_lower).as_str().to_owned(),
                severity: severity.as_str().to_owned(),
                marker: marker.clone(),
                snippet: trim_snippet(line, 120),
            });
        }
    }
    hits
}

struct CachePaths {
    directory: PathBuf,
    current: PathBuf,
    lock: PathBuf,
}

fn cache_paths(workspace_root: &Path, scope_alias: &str, scope_root: &Path) -> CachePaths {
    let mut hasher = Sha256::new();
    hasher.update(scope_root.to_string_lossy().as_bytes());
    let digest = hex_digest(hasher.finalize().as_slice());
    let safe_alias = scope_alias
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let directory = workspace_root
        .join(".effigy/doctor/cache/v1")
        .join(format!("{safe_alias}-{}", &digest[..12]));
    CachePaths {
        current: directory.join("generation.json"),
        lock: directory.join("publish.lock"),
        directory,
    }
}

fn config_identity(
    scope_root: &Path,
    pruned_roots: &[PathBuf],
    options: &DoctorScanInventoryOptions,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"effigy.doctor.cache.v1\0scanner-implementation-v1\0");
    hasher.update(scope_root.to_string_lossy().as_bytes());
    for root in pruned_roots {
        hasher.update(b"\0prune\0");
        hasher.update(root.to_string_lossy().as_bytes());
    }
    hasher.update(format!("\0{options:?}").as_bytes());
    format!("sha256:{}", hex_digest(hasher.finalize().as_slice()))
}

fn content_identity(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!(
        "content:sha256:{}",
        hex_digest(hasher.finalize().as_slice())
    )
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn load_cache(
    path: &Path,
    config_identity: &str,
    refresh: bool,
) -> (BTreeMap<String, Observation>, usize, Vec<String>) {
    if refresh || !path.is_file() {
        return (BTreeMap::new(), 0, Vec::new());
    }
    let source = match std::fs::read(path) {
        Ok(source) => source,
        Err(error) => {
            return (
                BTreeMap::new(),
                1,
                vec![format!(
                    "doctor cache read failed for {} and will be rebuilt: {error}",
                    path.display()
                )],
            )
        }
    };
    let generation = match serde_json::from_slice::<CacheGeneration>(&source) {
        Ok(generation) => generation,
        Err(error) => {
            return (
                BTreeMap::new(),
                1,
                vec![format!(
                    "doctor cache content at {} is corrupt and will be rebuilt: {error}",
                    path.display()
                )],
            )
        }
    };
    if generation.schema != "effigy.doctor.cache.v1"
        || generation.config_identity != config_identity
    {
        return (
            BTreeMap::new(),
            1,
            vec![format!(
                "doctor cache content at {} is incompatible and will be rebuilt",
                path.display()
            )],
        );
    }
    (generation.files, 0, Vec::new())
}

fn publish_cache(paths: &CachePaths, generation: &CacheGeneration) -> Result<(), String> {
    std::fs::create_dir_all(&paths.directory).map_err(|error| {
        format!(
            "doctor cache directory creation failed for {}: {error}",
            paths.directory.display()
        )
    })?;
    let lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&paths.lock)
        .map_err(|error| {
            format!(
                "doctor cache publication lock unavailable at {}: {error}",
                paths.lock.display()
            )
        })?;
    let result = publish_cache_locked(paths, generation);
    drop(lock);
    let _ = std::fs::remove_file(&paths.lock);
    result
}

fn publish_cache_locked(paths: &CachePaths, generation: &CacheGeneration) -> Result<(), String> {
    let encoded = serde_json::to_vec(generation)
        .map_err(|error| format!("doctor cache serialization failed: {error}"))?;
    let temporary = paths.directory.join(format!(
        ".generation-{}-{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| {
            format!(
                "doctor cache temporary file creation failed for {}: {error}",
                temporary.display()
            )
        })?;
    let result = (|| {
        file.write_all(&encoded)
            .map_err(|error| format!("doctor cache temporary write failed: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("doctor cache temporary sync failed: {error}"))?;
        std::fs::rename(&temporary, &paths.current).map_err(|error| {
            format!(
                "doctor cache atomic publication failed from {} to {}: {error}",
                temporary.display(),
                paths.current.display()
            )
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn git_identities(scope_root: &Path) -> BTreeMap<String, String> {
    let tracked = Command::new("git")
        .args([
            "-C",
            &scope_root.display().to_string(),
            "ls-files",
            "--stage",
            "-z",
            "--",
            ".",
        ])
        .output();
    let Ok(tracked) = tracked else {
        return BTreeMap::new();
    };
    if !tracked.status.success() {
        return BTreeMap::new();
    }
    let dirty = dirty_git_paths(scope_root);
    tracked
        .stdout
        .split(|byte| *byte == 0)
        .filter_map(|record| {
            let rendered = String::from_utf8_lossy(record);
            let (header, path) = rendered.split_once('\t')?;
            if dirty.contains(path) {
                return None;
            }
            let blob = header.split_whitespace().nth(1)?;
            Some((path.to_owned(), format!("git-blob:{blob}")))
        })
        .collect()
}

fn dirty_git_paths(scope_root: &Path) -> BTreeSet<String> {
    let status = Command::new("git")
        .args([
            "-C",
            &scope_root.display().to_string(),
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ])
        .output();
    let Ok(status) = status else {
        return BTreeSet::new();
    };
    if !status.status.success() {
        return BTreeSet::new();
    }
    status
        .stdout
        .split(|byte| *byte == 0)
        .filter_map(|record| {
            let rendered = String::from_utf8_lossy(record);
            if rendered.len() >= 4 && rendered.as_bytes().get(2) == Some(&b' ') {
                Some(rendered[3..].to_owned())
            } else if !rendered.is_empty() {
                Some(rendered.into_owned())
            } else {
                None
            }
        })
        .collect()
}

fn generated_in_src_category(value: &str) -> GeneratedInSrcCategory {
    match value {
        "content-marker" => GeneratedInSrcCategory::ContentMarker,
        "generated-filename" => GeneratedInSrcCategory::GeneratedFilename,
        "generated-path" => GeneratedInSrcCategory::GeneratedPath,
        "bundled-artifact" => GeneratedInSrcCategory::BundledArtifact,
        _ => GeneratedInSrcCategory::ContentMarker,
    }
}

fn attention_category(value: &str) -> AttentionMarkerCategory {
    match value {
        "deprecation" => AttentionMarkerCategory::Deprecation,
        "temporary-artifact" => AttentionMarkerCategory::TemporaryArtifact,
        _ => AttentionMarkerCategory::DeferredWork,
    }
}

fn attention_severity(value: &str) -> AttentionMarkerSeverity {
    match value {
        "critical" => AttentionMarkerSeverity::Critical,
        "high" => AttentionMarkerSeverity::High,
        _ => AttentionMarkerSeverity::Warning,
    }
}

fn stale_category(value: &str) -> StaleSuppressionCategory {
    match value {
        "lint-disable" => StaleSuppressionCategory::LintDisable,
        "tool-bypass" => StaleSuppressionCategory::ToolBypass,
        _ => StaleSuppressionCategory::TypeIgnore,
    }
}

fn stale_severity(value: &str) -> StaleSuppressionSeverity {
    match value {
        "critical" => StaleSuppressionSeverity::Critical,
        "high" => StaleSuppressionSeverity::High,
        _ => StaleSuppressionSeverity::Warning,
    }
}

fn validate_options(options: &DoctorScanInventoryOptions) -> Result<(), ScanError> {
    if options.god_files.doctor_enabled {
        options.god_files.validate()?;
    }
    if options.duplicate_blocks.doctor_enabled {
        options.duplicate_blocks.validate()?;
    }
    if options.comment_ratio.doctor_enabled {
        options.comment_ratio.validate()?;
    }
    if options.generated_assets.doctor_enabled {
        options.generated_assets.validate()?;
    }
    if options.generated_in_src.doctor_enabled {
        options.generated_in_src.validate()?;
    }
    if options.attention_markers.doctor_enabled {
        options.attention_markers.validate()?;
    }
    if options.stale_suppressions.doctor_enabled {
        options.stale_suppressions.validate()?;
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
        (
            options.comment_ratio.doctor_enabled,
            options.comment_ratio.respect_gitignore,
        ),
        (
            options.generated_assets.doctor_enabled,
            options.generated_assets.respect_gitignore,
        ),
        (
            options.generated_in_src.doctor_enabled,
            options.generated_in_src.respect_gitignore,
        ),
        (
            options.attention_markers.doctor_enabled,
            options.attention_markers.respect_gitignore,
        ),
        (
            options.stale_suppressions.doctor_enabled,
            options.stale_suppressions.respect_gitignore,
        ),
    ]
    .into_iter()
    .filter_map(|(enabled, posture)| enabled.then_some(posture))
}

fn ensure_budget(deadline: Option<Instant>) -> Result<(), ScanError> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(ScanError::invocation(
            "doctor scan budget exhausted during inventory",
        ));
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
        if let Some(severity) = classify_severity(code_lines, &options.thresholds) {
            findings.push(GodFileFinding {
                path: observation.rel_str.clone(),
                code_lines,
                total_lines: observation.total_lines,
                severity,
                graph: None,
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
        thresholds: options.thresholds.clone(),
    })
}

fn evaluate_comment_ratio(
    root: &Path,
    observations: &[Observation],
    options: &CommentRatioScanOptions,
    deadline: Option<Instant>,
) -> Result<CommentRatioScanResult, ScanError> {
    let mut findings = Vec::new();
    let mut scanned_files = 0;
    let mut candidate_files = 0;
    for observation in observations {
        ensure_budget(deadline)?;
        if !observation.comment_ratio_eligible || observation.generated {
            continue;
        }
        scanned_files += 1;
        if observation.code_lines < options.thresholds.min_code_lines {
            continue;
        }
        candidate_files += 1;
        let ratio = observation.comment_lines as f64 / observation.code_lines as f64;
        if let Some(severity) = classify_comment_ratio_severity(ratio, &options.thresholds) {
            findings.push(CommentRatioFinding {
                path: observation.rel_str.clone(),
                code_lines: observation.code_lines,
                comment_lines: observation.comment_lines,
                ratio,
                severity,
            });
        }
    }
    findings.sort_by(|left, right| {
        comment_ratio_severity_rank(right.severity)
            .cmp(&comment_ratio_severity_rank(left.severity))
            .then_with(|| right.ratio.total_cmp(&left.ratio))
            .then_with(|| right.comment_lines.cmp(&left.comment_lines))
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(CommentRatioScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_files,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

fn evaluate_generated_assets(
    root: &Path,
    observations: &[Observation],
    options: &GeneratedAssetScanOptions,
    deadline: Option<Instant>,
) -> Result<GeneratedAssetScanResult, ScanError> {
    let mut findings = Vec::new();
    let mut scanned_files = 0;
    let mut candidate_files = 0;
    for observation in observations {
        ensure_budget(deadline)?;
        if !observation.generated_asset_eligible {
            continue;
        }
        scanned_files += 1;
        let Some(reason) = observation.generated_asset_reason.clone() else {
            continue;
        };
        candidate_files += 1;
        if let Some(severity) =
            classify_generated_asset_severity(observation.bytes, &options.thresholds)
        {
            findings.push(GeneratedAssetFinding {
                path: observation.rel_str.clone(),
                bytes: observation.bytes,
                severity,
                reason,
            });
        }
    }
    findings.sort_by(|left, right| {
        generated_asset_severity_rank(right.severity)
            .cmp(&generated_asset_severity_rank(left.severity))
            .then_with(|| right.bytes.cmp(&left.bytes))
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(GeneratedAssetScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_files,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

fn evaluate_generated_in_src(
    root: &Path,
    observations: &[Observation],
    options: &GeneratedInSrcScanOptions,
    deadline: Option<Instant>,
) -> Result<GeneratedInSrcScanResult, ScanError> {
    let mut findings = Vec::new();
    let mut scanned_files = 0;
    let mut candidate_files = 0;
    for observation in observations {
        ensure_budget(deadline)?;
        if !observation.generated_in_src_eligible {
            continue;
        }
        scanned_files += 1;
        let Some(generated) = observation.generated_in_src.as_ref() else {
            continue;
        };
        candidate_files += 1;
        if let Some(severity) =
            classify_generated_in_src_severity(observation.bytes, &options.thresholds)
        {
            findings.push(GeneratedInSrcFinding {
                path: observation.rel_str.clone(),
                category: generated_in_src_category(&generated.category),
                severity,
                reason: generated.reason.clone(),
                size_bytes: observation.bytes,
            });
        }
    }
    findings.sort_by(|left, right| {
        generated_in_src_severity_rank(right.severity)
            .cmp(&generated_in_src_severity_rank(left.severity))
            .then_with(|| {
                generated_in_src_category_rank(right.category)
                    .cmp(&generated_in_src_category_rank(left.category))
            })
            .then_with(|| right.size_bytes.cmp(&left.size_bytes))
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(GeneratedInSrcScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_files,
        findings,
        thresholds: options.thresholds.clone(),
        source_roots: options.source_roots.clone(),
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
        let lines = observation
            .normalized_lines
            .iter()
            .map(|line| NormalizedCodeLine {
                line_number: line.line_number,
                text: line.text.clone(),
            })
            .collect::<Vec<_>>();
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

fn evaluate_attention_markers(
    root: &Path,
    observations: &[Observation],
    options: &AttentionMarkerScanOptions,
    deadline: Option<Instant>,
) -> Result<AttentionMarkerScanResult, ScanError> {
    let mut findings = Vec::new();
    let mut scanned_files = 0;
    let mut matched_lines = 0;
    for observation in observations {
        ensure_budget(deadline)?;
        if !observation.attention_eligible || observation.generated {
            continue;
        }
        scanned_files += 1;
        let mut lines = BTreeSet::new();
        for hit in &observation.attention_hits {
            lines.insert(hit.line);
            findings.push(AttentionMarkerFinding {
                path: observation.rel_str.clone(),
                line: hit.line,
                category: attention_category(&hit.category),
                severity: attention_severity(&hit.severity),
                marker: hit.marker.clone(),
                snippet: hit.snippet.clone(),
                graph: None,
            });
        }
        matched_lines += lines.len();
    }
    findings.sort_by(|left, right| {
        attention_marker_severity_rank(right.severity)
            .cmp(&attention_marker_severity_rank(left.severity))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.marker.cmp(&right.marker))
    });
    Ok(AttentionMarkerScanResult {
        root: root.display().to_string(),
        scanned_files,
        matched_lines,
        findings,
        patterns: options.patterns.clone(),
    })
}

fn evaluate_stale_suppressions(
    root: &Path,
    observations: &[Observation],
    options: &StaleSuppressionScanOptions,
    deadline: Option<Instant>,
) -> Result<StaleSuppressionScanResult, ScanError> {
    let mut findings = Vec::new();
    let mut scanned_files = 0;
    let mut matched_lines = 0;
    for observation in observations {
        ensure_budget(deadline)?;
        if !observation.stale_eligible || observation.generated {
            continue;
        }
        scanned_files += 1;
        let mut lines = BTreeSet::new();
        for hit in &observation.stale_hits {
            lines.insert(hit.line);
            findings.push(StaleSuppressionFinding {
                path: observation.rel_str.clone(),
                line: hit.line,
                category: stale_category(&hit.category),
                severity: stale_severity(&hit.severity),
                marker: hit.marker.clone(),
                snippet: hit.snippet.clone(),
            });
        }
        matched_lines += lines.len();
    }
    findings.sort_by(|left, right| {
        stale_suppression_severity_rank(right.severity)
            .cmp(&stale_suppression_severity_rank(left.severity))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.marker.cmp(&right.marker))
    });
    Ok(StaleSuppressionScanResult {
        root: root.display().to_string(),
        scanned_files,
        matched_lines,
        findings,
        patterns: options.patterns.clone(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        run_attention_marker_scan_workspace, run_comment_ratio_scan_workspace,
        run_duplicate_block_scan_workspace, run_generated_asset_scan_workspace,
        run_generated_in_src_scan_workspace, run_god_file_scan_workspace,
        run_stale_suppression_scan_workspace,
    };

    fn options() -> DoctorScanInventoryOptions {
        DoctorScanInventoryOptions {
            god_files: GodFileScanOptions::default(),
            duplicate_blocks: DuplicateBlockScanOptions {
                doctor_enabled: true,
                ..DuplicateBlockScanOptions::default()
            },
            comment_ratio: CommentRatioScanOptions::default(),
            generated_assets: GeneratedAssetScanOptions::default(),
            generated_in_src: GeneratedInSrcScanOptions::default(),
            attention_markers: AttentionMarkerScanOptions::default(),
            stale_suppressions: StaleSuppressionScanOptions {
                doctor_enabled: true,
                ..StaleSuppressionScanOptions::default()
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

        let shared = run_doctor_scan_inventory(root, "root", root, &[], &options, true, None)
            .expect("shared inventory");

        assert_eq!(shared.physical_walks, 1);
        assert_eq!(
            shared.god_files,
            Some(
                run_god_file_scan_workspace(root, &[root.to_path_buf()], &options.god_files)
                    .expect("god files")
            )
        );
        assert_eq!(
            shared.comment_ratio,
            Some(
                run_comment_ratio_scan_workspace(
                    root,
                    &[root.to_path_buf()],
                    &options.comment_ratio,
                )
                .expect("comment ratio")
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
        assert_eq!(
            shared.generated_assets,
            Some(
                run_generated_asset_scan_workspace(
                    root,
                    &[root.to_path_buf()],
                    &options.generated_assets,
                )
                .expect("generated assets")
            )
        );
        assert_eq!(
            shared.generated_in_src,
            Some(
                run_generated_in_src_scan_workspace(
                    root,
                    &[root.to_path_buf()],
                    &options.generated_in_src,
                )
                .expect("generated in src")
            )
        );
        assert_eq!(
            shared.attention_markers,
            Some(
                run_attention_marker_scan_workspace(
                    root,
                    &[root.to_path_buf()],
                    &options.attention_markers,
                )
                .expect("attention markers")
            )
        );
        assert_eq!(
            shared.stale_suppressions,
            Some(
                run_stale_suppression_scan_workspace(
                    root,
                    &[root.to_path_buf()],
                    &options.stale_suppressions,
                )
                .expect("stale suppressions")
            )
        );
    }

    #[test]
    fn cache_reuses_exact_facts_and_same_size_edits_miss() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        let source = root.join("src/lib.rs");
        std::fs::write(&source, "fn alpha() {}\n").expect("source");
        let options = options();

        let cold = run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("cold inventory");
        assert_eq!(cold.cache_hits, 0);
        assert_eq!(cold.cache_misses, 1);

        let warm = run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("warm inventory");
        assert_eq!(warm.cache_hits, 1);
        assert_eq!(warm.cache_misses, 0);

        let refreshed = run_doctor_scan_inventory(root, "root", root, &[], &options, true, None)
            .expect("refreshed inventory");
        assert_eq!(refreshed.cache_hits, 0);
        assert_eq!(refreshed.cache_misses, 1);

        std::fs::write(&source, "fn bravo() {}\n").expect("same-size edit");
        let changed = run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("changed inventory");
        assert_eq!(changed.cache_hits, 0);
        assert_eq!(changed.cache_misses, 1);
    }

    #[test]
    fn corrupt_cache_warns_and_rebuilds_without_panicking() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("source");
        let options = options();
        run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("initial inventory");
        let paths = cache_paths(root, "root", root);
        std::fs::write(&paths.current, b"not-json").expect("corrupt cache");

        let rebuilt = run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("rebuilt inventory");
        assert_eq!(rebuilt.invalid_cache_entries, 1);
        assert_eq!(rebuilt.cache_misses, 1);
        assert!(rebuilt.warnings[0].contains("corrupt"));
    }

    #[test]
    fn incompatible_cache_version_warns_and_rebuilds() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("source");
        let options = options();
        run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("initial inventory");
        let paths = cache_paths(root, "root", root);
        let mut generation = serde_json::from_slice::<CacheGeneration>(
            &std::fs::read(&paths.current).expect("read cache"),
        )
        .expect("decode cache");
        generation.schema = "effigy.doctor.cache.v0".to_owned();
        std::fs::write(
            &paths.current,
            serde_json::to_vec(&generation).expect("encode incompatible cache"),
        )
        .expect("write incompatible cache");

        let rebuilt = run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("rebuilt inventory");
        assert_eq!(rebuilt.invalid_cache_entries, 1);
        assert_eq!(rebuilt.cache_misses, 1);
        assert!(rebuilt.warnings[0].contains("incompatible"));
    }

    #[test]
    fn pruned_sibling_is_not_observed_or_cached() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let sibling = root.join("apps/sibling");
        std::fs::create_dir_all(&sibling).expect("sibling");
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("root source");
        std::fs::write(sibling.join("secret.rs"), "fn sibling() {}\n").expect("sibling source");
        let options = options();

        let result = run_doctor_scan_inventory(
            root,
            "root",
            root,
            std::slice::from_ref(&sibling),
            &options,
            true,
            None,
        )
        .expect("root inventory");
        assert_eq!(result.god_files.expect("god files").scanned_files, 1);
    }

    #[test]
    fn expired_deadline_does_not_publish_partial_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("source");
        let options = options();
        let error = run_doctor_scan_inventory(
            root,
            "root",
            root,
            &[],
            &options,
            false,
            Some(Instant::now()),
        )
        .expect_err("deadline should fail");
        assert!(error.to_string().contains("budget exhausted"));
        assert!(!cache_paths(root, "root", root).current.exists());
    }
}
