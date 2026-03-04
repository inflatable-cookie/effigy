use crate::ui::{KeyValue, PlainRenderer, Renderer};

pub(super) const DOCTOR_REPORT_HEADING: &str = "Doctor's Report";
pub(super) const DOCTOR_EXPLAIN_HEADING: &str = "Doctor Explain";

pub(super) struct BulletListSection {
    pub(super) label: String,
    pub(super) items: Vec<String>,
}

pub(super) fn key_values_from_pairs(rows: Vec<(String, String)>) -> Vec<KeyValue> {
    rows.into_iter()
        .map(|(key, value)| KeyValue::new(key, value))
        .collect::<Vec<KeyValue>>()
}

pub(super) fn bullet_section(label: impl Into<String>, items: Vec<String>) -> BulletListSection {
    BulletListSection {
        label: label.into(),
        items,
    }
}

pub(super) fn optional_bullet_section(
    label: impl Into<String>,
    items: &[String],
) -> Option<BulletListSection> {
    if items.is_empty() {
        return None;
    }
    Some(BulletListSection {
        label: label.into(),
        items: items.to_vec(),
    })
}

pub(super) fn render_key_values(
    renderer: &mut PlainRenderer<Vec<u8>>,
    rows: &[KeyValue],
) -> Result<(), crate::ui::UiError> {
    renderer.key_values(rows)
}

pub(super) fn render_bullet_sections(
    renderer: &mut PlainRenderer<Vec<u8>>,
    sections: &[BulletListSection],
) -> Result<(), crate::ui::UiError> {
    for section in sections {
        renderer.bullet_list(&section.label, &section.items)?;
    }
    Ok(())
}
