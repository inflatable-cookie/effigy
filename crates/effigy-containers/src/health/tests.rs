use super::*;

#[test]
fn parse_http_health_check_with_path() {
    assert_eq!(
        parse_http_health_check("http://localhost:8080/health"),
        Some(("localhost".to_owned(), 8080, "/health".to_owned()))
    );
}

#[test]
fn parse_http_health_check_pathless() {
    assert_eq!(
        parse_http_health_check("http://localhost:8080"),
        Some(("localhost".to_owned(), 8080, "/".to_owned()))
    );
}

#[test]
fn parse_http_health_check_default_port() {
    assert_eq!(
        parse_http_health_check("http://localhost"),
        Some(("localhost".to_owned(), 80, "/".to_owned()))
    );
}

#[test]
fn parse_http_health_check_not_http() {
    assert_eq!(parse_http_health_check("tcp://localhost:3306"), None);
}

#[test]
fn parse_tcp_health_check_with_port() {
    assert_eq!(
        parse_tcp_health_check("tcp://127.0.0.1:5432"),
        Some(("127.0.0.1".to_owned(), 5432))
    );
}

#[test]
fn parse_tcp_health_check_no_port_returns_none() {
    assert_eq!(parse_tcp_health_check("tcp://localhost"), None);
}

#[test]
fn parse_tcp_health_check_not_tcp() {
    assert_eq!(parse_tcp_health_check("http://localhost:80"), None);
}

#[test]
fn split_host_port_with_explicit_port() {
    assert_eq!(
        split_host_port("localhost:3000", 80),
        Some(("localhost".to_owned(), 3000))
    );
}

#[test]
fn split_host_port_uses_default() {
    assert_eq!(
        split_host_port("localhost", 80),
        Some(("localhost".to_owned(), 80))
    );
}

#[test]
fn split_host_port_requires_port_when_default_zero() {
    assert_eq!(split_host_port("localhost", 0), None);
}

#[test]
fn probe_health_status_none_returns_none() {
    assert_eq!(probe_health_status(None), None);
}
