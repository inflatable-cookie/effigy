use super::context::refresh_progress_message;
use effigy_codegraph::RefreshPending;

#[test]
fn refresh_notice_claims_a_cold_build() {
    let notice = refresh_progress_message(RefreshPending::Cold).expect("cold must announce");
    assert!(notice.contains("missing"), "cold notice: {notice}");
    assert!(notice.contains("docs context"), "cold notice: {notice}");
}

#[test]
fn refresh_notice_claims_a_stale_rebuild() {
    let notice = refresh_progress_message(RefreshPending::Stale).expect("stale must announce");
    assert!(notice.contains("stale"), "stale notice: {notice}");
    assert!(notice.contains("docs context"), "stale notice: {notice}");
}

#[test]
fn refresh_notice_stays_silent_when_current() {
    assert_eq!(refresh_progress_message(RefreshPending::Current), None);
}
