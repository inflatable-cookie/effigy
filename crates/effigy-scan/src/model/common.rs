use effigy_manifest::config_sections::ManifestScanOutputFormat;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanRenderFormat {
    Text,
    Markdown,
}

impl ScanRenderFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Markdown => "markdown",
        }
    }
}

impl From<ManifestScanOutputFormat> for ScanRenderFormat {
    fn from(value: ManifestScanOutputFormat) -> Self {
        match value {
            ManifestScanOutputFormat::Text => Self::Text,
            ManifestScanOutputFormat::Markdown => Self::Markdown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRenderOptions {
    pub show_warnings: bool,
    pub color_enabled: bool,
}

pub fn format_bytes(bytes: usize) -> String {
    if bytes >= 1_000_000_000 {
        return format!("{:.1} GB", bytes as f64 / 1_000_000_000f64);
    }
    if bytes >= 1_000_000 {
        return format!("{:.1} MB", bytes as f64 / 1_000_000f64);
    }
    if bytes >= 1_000 {
        return format!("{:.1} KB", bytes as f64 / 1_000f64);
    }
    format!("{bytes} B")
}

pub fn format_ratio(ratio: f64) -> String {
    format!("{ratio:.2}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanGraphFileContext {
    pub language_id: String,
    pub symbol_count: usize,
    pub inbound_edges: usize,
    pub outbound_edges: usize,
    pub reference_count: usize,
    pub connectivity: String,
}
