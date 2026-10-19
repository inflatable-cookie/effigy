//! Calendar dates for bounded QA group lifecycle metadata.
use chrono::NaiveDate;

/// Strict `YYYY-MM-DD` lifecycle date used by QA group lifecycle metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ManifestLifecycleDate(NaiveDate);

impl ManifestLifecycleDate {
    /// Parse one strict zero-padded `YYYY-MM-DD` calendar date.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if raw.len() != 10 {
            return Err(format!(
                "`{raw}` is not a strict `YYYY-MM-DD` date; use exactly ten characters such as `2026-09-15`"
            ));
        }
        let bytes = raw.as_bytes();
        let shaped = bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
        if !shaped {
            return Err(format!(
                "`{raw}` is not a strict `YYYY-MM-DD` date; use exactly ten characters such as `2026-09-15`"
            ));
        }
        NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .map(Self)
            .map_err(|_| format!("`{raw}` is not a real calendar date"))
    }

    pub fn to_naive_date(self) -> NaiveDate {
        self.0
    }

    /// Canonical zero-padded rendering.
    pub fn as_iso(self) -> String {
        self.0.format("%Y-%m-%d").to_string()
    }

    /// Today's date in the local calendar, used by production inventory and
    /// doctor surfaces. Tests inject an explicit date instead.
    pub fn today_local() -> Self {
        Self(chrono::Local::now().date_naive())
    }
}

impl std::fmt::Display for ManifestLifecycleDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_iso())
    }
}
