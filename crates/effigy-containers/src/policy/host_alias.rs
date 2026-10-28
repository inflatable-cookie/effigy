//! Scoped resolution of the canonical declared backing-service alias for
//! Effigy-owned host execution.
//!
//! Contract 005 gives a scoped checkout's container targets two names for one
//! backing service: the canonical declared alias application configuration
//! carries (`postgres.<apex>`) and the scope's effective alias
//! (`postgres.<apex>-w<key>`). A host process cannot be given the container
//! network alias: it resolves the declared name through the gateway, where the
//! route for that name belongs to whoever published it — normally the primary
//! checkout (effigy#264).
//!
//! Effigy-owned host execution therefore resolves the canonical declared alias
//! in the environment it delivers to the run, before the child starts: the
//! value the child receives names this checkout's own backing service, and the
//! consumer's manifest keeps the declared name. The mapping is the same
//! declared/effective pair the host map, the gateway registration and the
//! Compose network aliases already use, so host execution introduces no second
//! naming scheme. A primary checkout's declared and effective names are equal,
//! so nothing is rewritten there.

use super::hosts::{EffectiveHostMap, HostRouteKind};

/// One declared backing-service alias rewritten to this checkout's effective
/// alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredAlias {
    /// The canonical declared host application configuration carries.
    pub declared: String,
    /// The alias this checkout's own route and backing service answer on.
    pub effective: String,
    /// The backing service the alias names.
    pub service: String,
}

/// The declared-to-effective alias pairs a scoped checkout resolves in
/// Effigy-owned host execution. Empty for a checkout that keeps its declared
/// names (the primary checkout, or one that shares runtime identity).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostExecutionAliases {
    aliases: Vec<DeclaredAlias>,
}

impl HostExecutionAliases {
    /// The TCP alias routes a scoped checkout rewrote. HTTP routes are not
    /// included: they stay the scope's own host-visible surface and
    /// application configuration is what feeds public URLs.
    pub fn for_scoped_checkout(host_map: &EffectiveHostMap) -> Self {
        let aliases = host_map
            .routes
            .iter()
            .filter(|route| route.kind == HostRouteKind::Tcp)
            .filter_map(|route| {
                let declared = route.declared.trim();
                let effective = route.effective.trim();
                if declared.is_empty() || effective.is_empty() || declared == effective {
                    return None;
                }
                Some(DeclaredAlias {
                    declared: declared.to_owned(),
                    effective: effective.to_owned(),
                    service: route.service.clone().unwrap_or_default(),
                })
            })
            .collect();
        Self { aliases }
    }

    pub fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }

    pub fn aliases(&self) -> &[DeclaredAlias] {
        &self.aliases
    }

    /// Resolve every declared alias host this value names.
    ///
    /// `None` means the value names no declared alias; an untouched value
    /// keeps its exact bytes. Otherwise the returned value carries this
    /// checkout's effective aliases and the aliases it used, in first-match
    /// order.
    pub fn resolve(&self, value: &str) -> Option<(String, Vec<DeclaredAlias>)> {
        let mut resolved = value.to_owned();
        let mut used: Vec<DeclaredAlias> = Vec::new();
        for alias in &self.aliases {
            let Some(rewritten) = rewrite_host_tokens(&resolved, &alias.declared, &alias.effective)
            else {
                continue;
            };
            resolved = rewritten;
            if !used.iter().any(|existing| existing == alias) {
                used.push(alias.clone());
            }
        }
        (!used.is_empty()).then_some((resolved, used))
    }
}

/// Rewrite every whole-hostname occurrence of `declared` to `effective`.
///
/// Only a complete host is rewritten: `postgres.acme.test` is a match in
/// `postgres://user@postgres.acme.test:5432/db` and as a bare value, but never
/// inside `notpostgres.acme.test` or `postgres.acme.test.example.com`.
fn rewrite_host_tokens(value: &str, declared: &str, effective: &str) -> Option<String> {
    if declared.is_empty() || declared == effective || value.len() < declared.len() {
        return None;
    }
    let mut output = String::with_capacity(value.len());
    let mut copied = 0;
    let mut index = 0;
    let mut matched = false;
    while index < value.len() {
        let candidate = value
            .get(index..)
            .and_then(|rest| rest.get(..declared.len()));
        if let Some(candidate) = candidate {
            if candidate.eq_ignore_ascii_case(declared)
                && host_boundary_before(value, index)
                && host_boundary_after(value, index + declared.len())
            {
                output.push_str(&value[copied..index]);
                output.push_str(effective);
                index += declared.len();
                copied = index;
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
    use crate::policy::hosts::{build_host_map, EffectiveHostRoute, HostRouteKind, HostScopeKind};

    fn host_map(declared_base: &str, effective_base: &str, scoped: bool) -> EffectiveHostMap {
        let tcp = |label: &str, service: &str| EffectiveHostRoute {
            kind: HostRouteKind::Tcp,
            declared: format!("{label}.{declared_base}"),
            effective: format!("{label}.{effective_base}"),
            tls: false,
            service: Some(service.to_owned()),
            origin: None,
        };
        EffectiveHostMap {
            kind: if scoped {
                HostScopeKind::Worktree
            } else {
                HostScopeKind::Primary
            },
            token: scoped.then(|| "abcdef123456".to_owned()),
            host_key: scoped.then(|| "abcdef12".to_owned()),
            shared_runtime_identity: false,
            declared_base: Some(declared_base.to_owned()),
            effective_base: Some(effective_base.to_owned()),
            base_domain_inferred: false,
            base_domain_warning: None,
            routes: vec![
                EffectiveHostRoute {
                    kind: HostRouteKind::Http,
                    declared: format!("app.{declared_base}"),
                    effective: format!("app.{effective_base}"),
                    tls: false,
                    service: Some("workspace".to_owned()),
                    origin: None,
                },
                tcp("postgres", "postgres"),
                tcp("redis", "redis"),
            ],
        }
    }

    #[test]
    fn a_scoped_checkout_resolves_every_declared_backing_service_alias() {
        let aliases = HostExecutionAliases::for_scoped_checkout(&host_map(
            "acme.test",
            "acme-wabcdef12.test",
            true,
        ));
        assert_eq!(
            aliases.aliases(),
            [
                DeclaredAlias {
                    declared: "postgres.acme.test".to_owned(),
                    effective: "postgres.acme-wabcdef12.test".to_owned(),
                    service: "postgres".to_owned(),
                },
                DeclaredAlias {
                    declared: "redis.acme.test".to_owned(),
                    effective: "redis.acme-wabcdef12.test".to_owned(),
                    service: "redis".to_owned(),
                },
            ],
            "only TCP backing-service aliases are resolved, never the HTTP surface"
        );
    }

    #[test]
    fn a_primary_checkout_resolves_nothing() {
        let aliases =
            HostExecutionAliases::for_scoped_checkout(&host_map("acme.test", "acme.test", false));
        assert!(aliases.is_empty());
        assert_eq!(
            aliases.resolve("postgres://app@postgres.acme.test:5432/app"),
            None
        );
    }

    #[test]
    fn a_declared_alias_is_rewritten_wherever_it_names_a_whole_host() {
        let aliases = HostExecutionAliases::for_scoped_checkout(&host_map(
            "acme.test",
            "acme-wabcdef12.test",
            true,
        ));
        for (value, expected) in [
            (
                "postgres://app:secret@postgres.acme.test:5432/app",
                "postgres://app:secret@postgres.acme-wabcdef12.test:5432/app",
            ),
            ("postgres.acme.test", "postgres.acme-wabcdef12.test"),
            (
                "host=postgres.acme.test port=5432",
                "host=postgres.acme-wabcdef12.test port=5432",
            ),
            (
                "postgres.acme.test,redis.acme.test",
                "postgres.acme-wabcdef12.test,redis.acme-wabcdef12.test",
            ),
            ("POSTGRES.ACME.TEST", "postgres.acme-wabcdef12.test"),
        ] {
            let (resolved, used) = aliases.resolve(value).expect("a declared alias");
            assert_eq!(resolved, expected, "{value}");
            assert!(!used.is_empty(), "{value}");
        }
    }

    #[test]
    fn a_longer_or_short_host_is_never_rewritten() {
        let aliases = HostExecutionAliases::for_scoped_checkout(&host_map(
            "acme.test",
            "acme-wabcdef12.test",
            true,
        ));
        for value in [
            "postgres.acme.test.example.com",
            "notpostgres.acme.test",
            "postgres.acme.testing",
            "postgres.acmeXtest",
            "postgres.acme-wabcdef12.test",
            "pg.acme.test",
        ] {
            assert_eq!(aliases.resolve(value), None, "{value}");
        }
    }

    #[test]
    fn the_host_map_a_primary_checkout_keeps_is_unchanged() {
        // The same manifest through the real host map: a primary checkout
        // declares and answers one name, so host execution rewrites nothing.
        let map = build_host_map(
            &[],
            &[crate::EffectiveServiceAlias {
                service: "postgres".to_owned(),
                domain_label: "postgres".to_owned(),
                container_port: 5432,
            }],
            &[],
            None,
            false,
            Some("acme.test"),
        );
        assert_eq!(map.kind, HostScopeKind::Primary);
        let aliases = HostExecutionAliases::for_scoped_checkout(&map);
        assert!(aliases.is_empty(), "{aliases:?}");
    }
}
