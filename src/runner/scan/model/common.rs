use crate::runner::manifest::config_sections::ManifestScanOutputFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum ScanRenderFormat {
    Text,
    Markdown,
}

impl ScanRenderFormat {
    pub(in crate::runner) fn as_str(self) -> &'static str {
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
pub(in crate::runner) struct TextRenderOptions {
    pub(in crate::runner) show_warnings: bool,
    pub(in crate::runner) color_enabled: bool,
}

pub(in crate::runner) fn format_bytes(bytes: usize) -> String {
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

pub(in crate::runner) fn format_ratio(ratio: f64) -> String {
    format!("{ratio:.2}")
}
