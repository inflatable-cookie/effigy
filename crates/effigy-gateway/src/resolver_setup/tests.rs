use super::*;

#[test]
fn resolver_path_is_correct() {
    assert_eq!(resolver_path("test"), PathBuf::from("/etc/resolver/test"));
    assert_eq!(resolver_path("dev"), PathBuf::from("/etc/resolver/dev"));
}

#[test]
fn resolver_file_contents_format() {
    let content = resolver_file_contents(15353);
    assert!(content.contains("nameserver 127.0.0.1"));
    assert!(content.contains("port 15353"));
    assert!(content.contains("Effigy"));
}

#[test]
fn resolver_spec_has_correct_path_and_content() {
    let spec = resolver_file_spec("test", 15353);
    assert_eq!(spec.path, PathBuf::from("/etc/resolver/test"));
    assert!(spec.content.contains("port 15353"));
}

#[test]
fn resolver_spec_with_custom_port() {
    let spec = resolver_file_spec("dev", 5353);
    assert!(spec.content.contains("port 5353"));
}
