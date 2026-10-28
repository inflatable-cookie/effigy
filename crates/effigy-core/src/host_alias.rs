//! Whole-host rewriting of a declared backing-service alias (effigy#431).
//!
//! A runtime-scoped checkout answers its own backing service on an effective
//! alias (`postgres.<apex>-w<key>`) while the consumer's configuration keeps
//! the canonical declared name (`postgres.<apex>`). Effigy-owned host
//! execution delivers the effective name in the environment it hands a host
//! run, so the run reaches this checkout's own service instead of whichever
//! checkout the declared name's route belongs to (contract 005).
//!
//! The rewrite lives here so every host-side renderer — the runner's task
//! environment, the managed session's run specs and sequence steps — applies
//! the one rule rather than growing its own.
//!
//! Only an endpoint-shaped occurrence is rewritten: the whole value, the host
//! of a URL authority (after `//` or `@`), or a host carrying its own port
//! (`host:5432`). A declared name that merely appears inside other text — a
//! search pattern, a JSON blob, a list of names, a note — keeps its exact
//! bytes, because Effigy cannot know that the text is meant as an endpoint.

use std::collections::BTreeMap;

/// One declared backing-service alias rewritten to the checkout's effective
/// alias.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HostAlias {
    /// The canonical declared host application configuration carries.
    pub declared: String,
    /// The alias this checkout's own route and backing service answer on.
    pub effective: String,
}

impl HostAlias {
    /// The alias a scoped checkout resolves, or `None` when the two names are
    /// equal or either is empty: a checkout that keeps its declared names
    /// rewrites nothing.
    pub fn scoped(declared: impl Into<String>, effective: impl Into<String>) -> Option<Self> {
        let declared = declared.into();
        let effective = effective.into();
        let declared = declared.trim();
        let effective = effective.trim();
        if declared.is_empty() || effective.is_empty() || declared == effective {
            return None;
        }
        Some(Self {
            declared: declared.to_owned(),
            effective: effective.to_owned(),
        })
    }

    /// Resolve every endpoint-shaped occurrence of the declared host in one
    /// value. `None` means the value is unchanged.
    pub fn resolve(&self, value: &str) -> Option<String> {
        rewrite_endpoints(value, &self.declared, &self.effective)
    }

    /// Whether `text` names this alias's effective host as an endpoint: the
    /// whole value, a host in a URL authority (`//host`, `@host`), an
    /// assignment (`KEY=host`) or a host carrying its own port
    /// (`host:5432`).
    ///
    /// This is the read-side twin of [`Self::resolve`]: it proves exactly the
    /// aliases Effigy delivered, and never fires on text that merely mentions
    /// the name — a search pattern, a JSON blob, a list, a note — or on a
    /// longer host.
    pub fn names_endpoint(&self, text: &str) -> bool {
        let whole_value = text.trim();
        if whole_value.eq_ignore_ascii_case(&self.effective) {
            return true;
        }
        if self.effective.is_empty() || text.len() < self.effective.len() {
            return false;
        }
        let mut index = 0;
        while index < text.len() {
            let candidate = text
                .get(index..)
                .and_then(|rest| rest.get(..self.effective.len()));
            if let Some(candidate) = candidate {
                let end = index + self.effective.len();
                if candidate.eq_ignore_ascii_case(&self.effective)
                    && host_boundary_before(text, index)
                    && host_boundary_after(text, end)
                    && is_endpoint_occurrence(text, index, end, whole_value, &self.effective)
                {
                    return true;
                }
            }
            index += text[index..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(1);
        }
        false
    }
}

/// Resolve every alias in one value. `None` means the value is unchanged.
pub fn resolve_value(value: &str, aliases: &[HostAlias]) -> Option<String> {
    let mut resolved = value.to_owned();
    let mut changed = false;
    for alias in aliases {
        if let Some(rewritten) = alias.resolve(&resolved) {
            resolved = rewritten;
            changed = true;
        }
    }
    changed.then_some(resolved)
}

/// Resolve every endpoint-shaped value in one environment map in place.
pub fn resolve_env(env: &mut BTreeMap<String, String>, aliases: &[HostAlias]) {
    if aliases.is_empty() {
        return;
    }
    for value in env.values_mut() {
        if let Some(resolved) = resolve_value(value, aliases) {
            *value = resolved;
        }
    }
}

/// Rewrite every endpoint-shaped occurrence of `declared` to `effective`.
fn rewrite_endpoints(value: &str, declared: &str, effective: &str) -> Option<String> {
    if declared.is_empty() || declared == effective || value.len() < declared.len() {
        return None;
    }
    let whole_value = value.trim();
    let mut output = String::new();
    let mut copied = 0;
    let mut index = 0;
    let mut matched = false;
    while index < value.len() {
        let candidate = value
            .get(index..)
            .and_then(|rest| rest.get(..declared.len()));
        if let Some(candidate) = candidate {
            let end = index + declared.len();
            if candidate.eq_ignore_ascii_case(declared)
                && host_boundary_before(value, index)
                && host_boundary_after(value, end)
                && is_rewritten_endpoint(value, index, end, whole_value, declared)
            {
                output.push_str(&value[copied..index]);
                output.push_str(effective);
                index = end;
                copied = end;
                matched = true;
                continue;
            }
        }
        index += value[index..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);
    }
    if !matched {
        return None;
    }
    output.push_str(&value[copied..]);
    Some(output)
}

/// Whether the whole-hostname match at `[index, end)` is an endpoint a value
/// may be rewritten from: the whole value, a URL authority's host, or a host
/// carrying its own port.
///
/// A URL's userinfo is never an endpoint. `postgres://user:password@host` and
/// `postgres://user:123@host` both put a name and a number where a host and a
/// port would otherwise sit, so a match before the authority's last `@` is
/// credentials, whatever surrounds it.
fn is_endpoint_occurrence(
    value: &str,
    index: usize,
    end: usize,
    whole_value: &str,
    declared: &str,
) -> bool {
    is_rewritten_endpoint(value, index, end, whole_value, declared)
        || (!is_url_userinfo(value, index, end) && is_assignment(value, index))
}

/// The rewrite's own rule: prose keeps its bytes, so an assignment-shaped
/// `KEY=host` is only an endpoint once Effigy has rendered it into a command
/// (see [`Self::names_endpoint`]).
fn is_rewritten_endpoint(
    value: &str,
    index: usize,
    end: usize,
    whole_value: &str,
    declared: &str,
) -> bool {
    if whole_value.eq_ignore_ascii_case(declared) {
        return true;
    }
    if is_authority_host(value, index, end) {
        return true;
    }
    !is_url_userinfo(value, index, end) && carries_a_port(value, end)
}

/// A match inside a URL's userinfo: the authority before it is open and another
/// `@` follows in the same authority, so a host cannot be what this names.
fn is_url_userinfo(value: &str, index: usize, end: usize) -> bool {
    let before = &value[..index];
    let Some(scheme_end) = before.rfind("://") else {
        return false;
    };
    let authority = &before[scheme_end + 3..];
    if authority
        .chars()
        .any(|character| matches!(character, '/' | '?' | '#') || character.is_whitespace())
    {
        return false;
    }
    has_later_authority_at(value, end)
}

/// Whether the authority's host segment after `end` still carries an `@`, which
/// means everything before it is the userinfo.
fn has_later_authority_at(value: &str, end: usize) -> bool {
    value[end..]
        .split(['/', '?', '#', '"', '\'', '`', ' ', ',', ';'])
        .next()
        .unwrap_or_default()
        .contains('@')
}

/// The host part of a URL authority: the component after `scheme://`, or after
/// the userinfo's last `@`.
///
/// URL credentials are never the host. A name that appears in
/// `user:password@` — as either half of the userinfo — is left alone, so a task
/// pointing at an unrelated server keeps its bytes and proves nothing.
fn is_authority_host(value: &str, index: usize, end: usize) -> bool {
    let before = &value[..index];
    let Some(scheme_end) = before.rfind("://") else {
        return false;
    };
    let authority = &before[scheme_end + 3..];
    // The authority ends at the first path, query or whitespace.
    if authority
        .chars()
        .any(|character| matches!(character, '/' | '?' | '#') || character.is_whitespace())
    {
        return false;
    }
    // The host follows the authority start or the userinfo's last `@`.
    if !(authority.is_empty() || authority.ends_with('@')) {
        return false;
    }
    // A host never contains `@`, so another `@` in this authority's host
    // segment makes this match part of the userinfo instead.
    !has_later_authority_at(value, end)
}

/// `KEY=host` — an environment assignment, so the host is a value rather than
/// prose.
fn is_assignment(value: &str, index: usize) -> bool {
    value[..index].ends_with('=')
}

/// `host:<digits>` — the declaration that names a port, so the value is an
/// address rather than prose.
fn carries_a_port(value: &str, index: usize) -> bool {
    value[index..]
        .strip_prefix(':')
        .is_some_and(|rest| rest.starts_with(|character: char| character.is_ascii_digit()))
}

/// A match starts at the beginning of the value or after a character that
/// cannot be part of the preceding hostname.
fn host_boundary_before(value: &str, index: usize) -> bool {
    value[..index]
        .chars()
        .next_back()
        .is_none_or(|previous| !host_char(previous))
}

/// A match ends at the end of the value or before a character that cannot
/// continue the hostname, so a longer host is never truncated.
fn host_boundary_after(value: &str, index: usize) -> bool {
    value[index..]
        .chars()
        .next()
        .is_none_or(|next| !host_char(next))
}

fn host_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alias() -> HostAlias {
        HostAlias::scoped("postgres.acme.test", "postgres.acme-wabcdef12.test")
            .expect("a scoped alias")
    }

    #[test]
    fn an_equal_or_empty_pair_rewrites_nothing() {
        assert_eq!(
            HostAlias::scoped("postgres.acme.test", "postgres.acme.test"),
            None
        );
        assert_eq!(HostAlias::scoped("", "postgres.acme-w1.test"), None);
        assert_eq!(HostAlias::scoped("postgres.acme.test", " "), None);
        let alias = alias();
        assert_eq!(
            HostAlias::scoped(" postgres.acme.test ", "postgres.acme-wabcdef12.test"),
            Some(alias.clone())
        );
    }

    #[test]
    fn every_endpoint_shape_resolves() {
        let alias = alias();
        for (value, expected) in [
            (
                "postgres://app:secret@postgres.acme.test:5432/app",
                "postgres://app:secret@postgres.acme-wabcdef12.test:5432/app",
            ),
            (
                "postgres://postgres.acme.test:5432/app",
                "postgres://postgres.acme-wabcdef12.test:5432/app",
            ),
            (
                "postgres://postgres.acme.test/app",
                "postgres://postgres.acme-wabcdef12.test/app",
            ),
            ("postgres.acme.test", "postgres.acme-wabcdef12.test"),
            ("  postgres.acme.test  ", "  postgres.acme-wabcdef12.test  "),
            (
                "postgres.acme.test:5432",
                "postgres.acme-wabcdef12.test:5432",
            ),
            (
                "POSTGRES.ACME.TEST:5432",
                "postgres.acme-wabcdef12.test:5432",
            ),
        ] {
            assert_eq!(alias.resolve(value).as_deref(), Some(expected), "{value}");
        }
    }

    #[test]
    fn an_endpoint_is_recognised_and_prose_is_not() {
        let alias = alias();
        for endpoint in [
            "postgres.acme-wabcdef12.test",
            "  postgres.acme-wabcdef12.test  ",
            "postgres://app@postgres.acme-wabcdef12.test:5432/app",
            "postgres://postgres.acme-wabcdef12.test/app",
            "DB_HOST=postgres.acme-wabcdef12.test",
            "env 'DATABASE_URL=postgres://app@postgres.acme-wabcdef12.test:5432/app' sh -c true",
            "postgres.acme-wabcdef12.test:5432",
        ] {
            assert!(alias.names_endpoint(endpoint), "{endpoint}");
        }
        for prose in [
            "grep -r postgres.acme-wabcdef12.test fixtures/",
            "searching postgres.acme-wabcdef12.test now",
            "{\"host\":\"postgres.acme-wabcdef12.test\"}",
            "postgres.acme-wabcdef12.test,redis.acme-wabcdef12.test",
            "postgres.acme-wabcdef12.test.example.com",
            "notpostgres.acme-wabcdef12.test",
            "postgres.acme-wabcdef12.testing",
        ] {
            assert!(!alias.names_endpoint(prose), "{prose}");
        }
        assert!(!alias.names_endpoint("postgres.acme.test"));
    }

    #[test]
    fn url_credentials_are_never_the_host() {
        let alias = alias();
        // The declared name in either half of the userinfo, and a URL that
        // points at an unrelated server, keep their exact bytes.
        for value in [
            "postgres://postgres.acme.test:secret@database.elsewhere.test:5432/app",
            "postgres://user:postgres.acme.test@database.elsewhere.test:5432/app",
            "postgres://postgres.acme.test@database.elsewhere.test:5432/app",
            "redis://:postgres.acme.test@cache.elsewhere.test:6379",
            "postgres://user@database.elsewhere.test:5432/app",
            // A numeric username and a numeric password read as `host:port`
            // unless the authority is read properly.
            "postgres://postgres.acme.test:123@other.example:5432/db",
            "postgres://user:postgres.acme.test:123@other.example:5432/db",
            "postgres://postgres.acme.test:5432@other.example:5432/db",
        ] {
            assert_eq!(alias.resolve(value), None, "{value}");
            assert!(!alias.names_endpoint(value), "{value}");
        }
        // The host after the userinfo is still the host.
        assert_eq!(
            alias
                .resolve("postgres://user:secret@postgres.acme.test:5432/app")
                .as_deref(),
            Some("postgres://user:secret@postgres.acme-wabcdef12.test:5432/app")
        );
        assert!(
            alias.names_endpoint("postgres://user:secret@postgres.acme-wabcdef12.test:5432/app")
        );
    }

    #[test]
    fn text_that_merely_names_the_declared_host_is_untouched() {
        let alias = alias();
        for value in [
            "grep -r postgres.acme.test fixtures/",
            "host=postgres.acme.test port=5432",
            "{\"host\":\"postgres.acme.test\"}",
            "postgres.acme.test,redis.acme.test",
            "snapshot of postgres.acme.test taken",
            "postgres.acme.test.example.com",
            "notpostgres.acme.test",
            "postgres.acme.testing",
            "postgres.acme.test:",
            "postgres.acme.test:notaport",
            "postgres.acme-wabcdef12.test",
        ] {
            assert_eq!(alias.resolve(value), None, "{value}");
        }
    }

    #[test]
    fn a_map_resolves_every_value_and_keeps_others() {
        let mut env = BTreeMap::from([
            (
                "DATABASE_URL".to_owned(),
                "postgres://app@postgres.acme.test:5432/app".to_owned(),
            ),
            ("DB_HOST".to_owned(), "postgres.acme.test".to_owned()),
            ("SEARCH".to_owned(), "postgres.acme.test".to_owned()),
            ("UNRELATED".to_owned(), "nothing to see".to_owned()),
        ]);
        resolve_env(&mut env, &[alias()]);
        assert_eq!(
            env["DATABASE_URL"],
            "postgres://app@postgres.acme-wabcdef12.test:5432/app"
        );
        assert_eq!(env["DB_HOST"], "postgres.acme-wabcdef12.test");
        assert_eq!(env["SEARCH"], "postgres.acme-wabcdef12.test");
        assert_eq!(env["UNRELATED"], "nothing to see");
        assert_eq!(resolve_value("postgres.acme.test", &[]), None);
    }
}
