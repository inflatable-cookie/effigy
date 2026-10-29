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
//!
//! The rewrite itself is [`effigy_core::host_alias`], shared with the managed
//! session's run specs; this module only decides which aliases a checkout has
//! and which backing service each names.

use effigy_core::host_alias::HostAlias;

use super::hosts::{EffectiveHostMap, HostRouteKind};

/// One declared backing-service alias rewritten to this checkout's effective
/// alias, with the service it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredAlias {
    pub host: HostAlias,
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
                Some(DeclaredAlias {
                    host: HostAlias::scoped(route.declared.as_str(), route.effective.as_str())?,
                    service: route.service.clone().unwrap_or_default(),
                })
            })
            .collect();
        Self { aliases }
    }

    /// The aliases already resolved for one checkout: a union across its
    /// environments, each pair carrying its own service.
    pub fn from_declared(aliases: Vec<DeclaredAlias>) -> Self {
        Self { aliases }
    }

    pub fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }

    pub fn aliases(&self) -> &[DeclaredAlias] {
        &self.aliases
    }

    /// The declared/effective pairs every host-side renderer applies.
    pub fn hosts(&self) -> Vec<HostAlias> {
        self.aliases
            .iter()
            .map(|alias| alias.host.clone())
            .collect()
    }

    /// Resolve every declared alias endpoint this value names.
    ///
    /// `None` means the value is unchanged. Otherwise the returned value
    /// carries this checkout's effective aliases and the aliases it used, in
    /// first-match order.
    pub fn resolve(&self, value: &str) -> Option<(String, Vec<DeclaredAlias>)> {
        let mut resolved = value.to_owned();
        let mut used: Vec<DeclaredAlias> = Vec::new();
        for alias in &self.aliases {
            let Some(rewritten) = alias.host.resolve(&resolved) else {
                continue;
            };
            resolved = rewritten;
            if !used.contains(alias) {
                used.push(alias.clone());
            }
        }
        (!used.is_empty()).then_some((resolved, used))
    }
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
                    host: HostAlias::scoped("postgres.acme.test", "postgres.acme-wabcdef12.test")
                        .expect("scoped alias"),
                    service: "postgres".to_owned(),
                },
                DeclaredAlias {
                    host: HostAlias::scoped("redis.acme.test", "redis.acme-wabcdef12.test")
                        .expect("scoped alias"),
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
    fn a_declared_alias_resolves_where_it_names_an_endpoint() {
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
                "postgres.acme.test:5432",
                "postgres.acme-wabcdef12.test:5432",
            ),
        ] {
            let (resolved, used) = aliases.resolve(value).expect("a declared alias");
            assert_eq!(resolved, expected, "{value}");
            assert_eq!(used.len(), 1, "{value}");
            assert_eq!(used[0].service, "postgres", "{value}");
        }
    }

    #[test]
    fn a_value_that_only_names_the_alias_is_left_alone() {
        let aliases = HostExecutionAliases::for_scoped_checkout(&host_map(
            "acme.test",
            "acme-wabcdef12.test",
            true,
        ));
        for value in [
            "grep -r postgres.acme.test fixtures/",
            "postgres.acme.test.example.com",
            "notpostgres.acme.test",
            "postgres.acme.test,redis.acme.test",
            "postgres.acme-wabcdef12.test",
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
