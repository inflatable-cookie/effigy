use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::CodeGraphError;
use crate::extractor::SourceFile;
use crate::model::{
    Confidence, FileIndexStatus, FileRecord, Provenance, SourcePosition, SourceSpan,
};
use crate::ExtractorId;

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn language_id_for_path(path: &str) -> Option<&'static str> {
    if path.ends_with(".rs") {
        Some("rust")
    } else if path == "effigy.toml" || path.ends_with(".toml") {
        Some("toml")
    } else if path.ends_with(".md") || path.ends_with("SKILL.md") {
        Some("markdown")
    } else if path.ends_with(".php") || path.ends_with(".phtml") {
        Some("php")
    } else if path.ends_with(".py") {
        Some("python")
    } else if path.ends_with(".tsx") {
        Some("tsx")
    } else if path.ends_with(".ts") {
        Some("typescript")
    } else if path.ends_with(".jsx") {
        Some("jsx")
    } else if path.ends_with(".js") || path.ends_with(".mjs") || path.ends_with(".cjs") {
        Some("javascript")
    } else {
        None
    }
}

pub fn file_record_from_source(source: &SourceFile) -> Result<FileRecord, CodeGraphError> {
    Ok(FileRecord {
        id: crate::extractor::file_graph_id(&source.relative_path)?,
        path: source.relative_path.clone(),
        content_hash: sha256_hex(source.content.as_bytes()),
        language_id: source.language_id.clone(),
        byte_size: source.content.len() as u64,
        status: FileIndexStatus::Indexed,
    })
}

pub fn full_span(source: &str) -> SourceSpan {
    let mut line = 1u32;
    let mut column = 0u32;
    let mut byte = 0u32;
    for ch in source.chars() {
        byte += ch.len_utf8() as u32;
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    SourceSpan {
        start: SourcePosition {
            line: 1,
            column: 0,
            byte: 0,
        },
        end: SourcePosition { line, column, byte },
    }
}

pub fn span_from_bytes(source: &str, start_byte: usize, end_byte: usize) -> SourceSpan {
    SourceSpan {
        start: position_from_byte(source, start_byte),
        end: position_from_byte(source, end_byte),
    }
}

pub fn position_from_byte(source: &str, target_byte: usize) -> SourcePosition {
    let mut line = 1u32;
    let mut column = 0u32;
    let mut byte = 0usize;
    for ch in source.chars() {
        if byte >= target_byte {
            break;
        }
        byte += ch.len_utf8();
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    SourcePosition {
        line,
        column,
        byte: target_byte as u32,
    }
}

pub fn provenance_for_file(
    extractor_id: &ExtractorId,
    extractor_version: &str,
    source: &SourceFile,
    confidence: Confidence,
    detail: Option<&str>,
) -> Provenance {
    Provenance {
        extractor_id: extractor_id.clone(),
        extractor_version: extractor_version.to_owned(),
        source_path: source.relative_path.clone(),
        confidence,
        detail: detail.map(str::to_owned),
    }
}

pub fn normalize_rel_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn id_fragment(value: &str) -> String {
    let mut fragment = String::new();
    let mut last_underscore = false;
    for ch in value.chars() {
        if ch.is_control() {
            continue;
        }
        if ch.is_whitespace() {
            if !last_underscore {
                fragment.push('_');
                last_underscore = true;
            }
            continue;
        }
        fragment.push(ch);
        last_underscore = false;
    }
    let fragment = fragment.trim_matches('_');
    if fragment.is_empty() {
        "empty".to_owned()
    } else {
        fragment.to_owned()
    }
}
