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

/// Percent-encode the characters a graph id may not carry literally.
///
/// Graph ids reject whitespace and control characters, so a repository path or
/// declared link destination containing them cannot be embedded in a record id
/// as written. `%` is escaped too, which makes the encoding reversible:
/// `docs/my file.md` becomes `docs/my%20file.md` while a literal
/// `docs/my%20file.md` becomes `docs/my%2520file.md`, so the two never collide.
/// Every other character passes through unchanged, so paths that never needed
/// encoding keep their historical record ids.
pub fn encode_graph_path(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    let mut buffer = [0u8; 4];
    for ch in value.chars() {
        if ch == '%' || ch.is_whitespace() || ch.is_control() {
            for byte in ch.encode_utf8(&mut buffer).as_bytes() {
                encoded.push('%');
                encoded.push_str(&format!("{byte:02X}"));
            }
        } else {
            encoded.push(ch);
        }
    }
    encoded
}

/// Inverse of [`encode_graph_path`] for recovery from record ids.
///
/// A `%` that does not start a valid `%XX` escape passes through unchanged, so
/// ids written before path encoding existed stay readable.
pub fn decode_graph_path(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                decoded.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_graph_path_is_identity_for_ordinary_paths() {
        assert_eq!(encode_graph_path("docs/guide.md"), "docs/guide.md");
        assert_eq!(
            encode_graph_path("docs/_draft/guide-1.md"),
            "docs/_draft/guide-1.md"
        );
        // Non-ASCII bytes stay literal: only rejected characters are escaped.
        assert_eq!(
            encode_graph_path("docs/guide-über.md"),
            "docs/guide-über.md"
        );
    }

    #[test]
    fn encode_graph_path_escapes_whitespace_control_and_percent() {
        assert_eq!(encode_graph_path("docs/my file.md"), "docs/my%20file.md");
        assert_eq!(encode_graph_path("a\tb.md"), "a%09b.md");
        assert_eq!(
            encode_graph_path("docs/my%20file.md"),
            "docs/my%2520file.md"
        );
    }

    #[test]
    fn graph_path_codec_roundtrips_and_keeps_lookalike_paths_distinct() {
        for path in ["docs/my file.md", "docs/my%20file.md", "docs/a\tb.md"] {
            assert_eq!(decode_graph_path(&encode_graph_path(path)), path);
        }
        // The encoded forms of the two lookalike paths above stay distinct,
        // and decoding them recovers each original exactly.
        assert_ne!(
            encode_graph_path("docs/my file.md"),
            encode_graph_path("docs/my%20file.md")
        );
        assert_eq!(decode_graph_path("docs/my%20file.md"), "docs/my file.md");
        assert_eq!(
            decode_graph_path("docs/my%2520file.md"),
            "docs/my%20file.md"
        );
    }

    #[test]
    fn decode_graph_path_leaves_invalid_escapes_alone() {
        assert_eq!(decode_graph_path("docs/a%2Z.md"), "docs/a%2Z.md");
        assert_eq!(decode_graph_path("docs/a%2.md"), "docs/a%2.md");
        assert_eq!(decode_graph_path("docs/100%.md"), "docs/100%.md");
    }
}
