use effigy_manifest::config_sections::ManifestScanOutputFormat;

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
