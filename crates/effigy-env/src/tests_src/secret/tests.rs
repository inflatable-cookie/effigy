use super::{ResolvedValue, SecretString};

#[test]
fn secret_display_is_redacted() {
    let secret = SecretString::new("hunter2".to_owned());
    assert_eq!(format!("{secret}"), "[REDACTED]");
}

#[test]
fn secret_debug_is_masked() {
    let secret = SecretString::new("hunter2".to_owned());
    assert_eq!(format!("{secret:?}"), "SecretString(****)");
}

#[test]
fn secret_zeroize_for_test_clears_visible_bytes() {
    let mut secret = SecretString::new("hunter2".to_owned());
    let original_len = secret.expose().len();
    assert_eq!(secret.bytes_for_test(), b"hunter2");

    secret.zeroize_for_test();

    assert!(secret.expose().is_empty());
    assert_eq!(
        secret.raw_bytes_for_test(original_len),
        &[0, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn resolved_value_plain_display() {
    let value = ResolvedValue::Plain("hello".to_owned());
    assert_eq!(format!("{value}"), "hello");
    assert!(!value.is_secret());
}

#[test]
fn resolved_value_secret_display() {
    let value = ResolvedValue::Secret(SecretString::new("secret".to_owned()));
    assert_eq!(format!("{value}"), "[REDACTED]");
    assert!(value.is_secret());
}
