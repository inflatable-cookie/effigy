//! Effective gateway hostnames for one runtime scope.
//!
//! The primary checkout keeps declared domains. A linked worktree or a marked
//! ephemeral clone that does not share runtime identity rewrites the apex so
//! every declared HTTP route and TCP alias stays together under
//! `<apex>-w<host-key>.<tld>`.

use super::model::{
    EffectiveContainerPolicy, EffectiveDnsRoute, EffectiveServiceAlias, SharedServiceBinding,
};
use effigy_core::worktree_scope;

pub const HOST_KEY_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostScopeKind {
    Primary,
    Worktree,
    /// Full clone marked `effigy.runtimeScope = ephemeral` in local config.
    EphemeralClone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveHostMap {
    pub kind: HostScopeKind,
    pub token: Option<String>,
    pub host_key: Option<String>,
    pub shared_runtime_identity: bool,
    pub declared_base: Option<String>,
    pub effective_base: Option<String>,
    pub routes: Vec<EffectiveHostRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveHostRoute {
    pub kind: HostRouteKind,
    pub declared: String,
    pub effective: String,
    pub tls: bool,
    pub service: Option<String>,
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostRouteKind {
    Http,
    Tcp,
}

impl EffectiveHostMap {
    pub fn cookie_domain(&self) -> Option<&str> {
        self.effective_base.as_deref()
    }

    pub fn relying_party_id(&self) -> Option<&str> {
        self.effective_base.as_deref()
    }

    /// Presentational override for which checkout shape owns the token.
    /// Route rewriting depends on the token, not the kind; callers that
    /// resolved the kind from the checkout call this so reports stay honest
    /// about ephemeral clones versus linked worktrees.
    pub fn with_scope_kind(mut self, kind: HostScopeKind) -> Self {
        self.kind = kind;
        self
    }
}

pub fn host_key(token: &str) -> Option<&str> {
    let token = token.trim();
    (token.len() >= HOST_KEY_LEN && token.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| &token[..HOST_KEY_LEN])
}

/// Resolve shared runtime identity from the effective project name and the
/// checkout's generation token. Scoped project names carry the generation
/// suffix; an unsuffixed project name intentionally shares its identity.
pub fn uses_shared_runtime_identity(
    policy: &EffectiveContainerPolicy,
    token: Option<&str>,
) -> bool {
    let Some(token) = token.filter(|token| token.len() >= 12) else {
        return false;
    };
    !worktree_scope::PROJECT_TAGS.iter().any(|tag| {
        policy
            .project_name
            .contains(&format!("-{tag}-{}", &token[..12]))
    })
}

pub fn rewrite_declared_host(declared: &str, host_key: &str) -> String {
    let parts = declared
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return declared.trim().to_owned();
    }
    let tld = parts[parts.len() - 1];
    let apex = parts[parts.len() - 2];
    let mut labels = parts[..parts.len() - 2]
        .iter()
        .map(|label| (*label).to_owned())
        .collect::<Vec<_>>();
    labels.push(format!("{apex}-w{host_key}"));
    labels.push(tld.to_owned());
    labels.join(".")
}

pub fn apply_scope_to_routes(routes: &mut [EffectiveDnsRoute], host_key: &str) {
    for route in routes {
        route.declared_domain = route.declared().to_owned();
        route.domain = rewrite_declared_host(&route.declared_domain, host_key);
    }
}

pub fn build_host_map(
    dns_routes: &[EffectiveDnsRoute],
    service_aliases: &[EffectiveServiceAlias],
    shared_services: &[SharedServiceBinding],
    token: Option<&str>,
    shared_runtime_identity: bool,
) -> EffectiveHostMap {
    let rewrite_key = token
        .filter(|_| !shared_runtime_identity)
        .and_then(host_key);
    let declared_base = dns_routes
        .first()
        .map(|route| route.declared().to_owned())
        .filter(|domain| domain.split('.').filter(|part| !part.is_empty()).count() >= 2);
    let effective_base = match (declared_base.as_deref(), rewrite_key) {
        (Some(declared), Some(key)) => Some(rewrite_declared_host(declared, key)),
        (Some(declared), None) => Some(declared.to_owned()),
        _ => None,
    };
    let mut routes = Vec::new();
    let mut occupied = std::collections::BTreeSet::new();
    for route in dns_routes {
        let declared = route.declared().to_owned();
        let effective = match rewrite_key {
            Some(key) => rewrite_declared_host(&declared, key),
            None => declared.clone(),
        };
        occupied.insert(declared.clone());
        occupied.insert(effective.clone());
        let origin = Some(if route.tls {
            format!("https://{effective}")
        } else {
            format!("http://{effective}")
        });
        routes.push(EffectiveHostRoute {
            kind: HostRouteKind::Http,
            declared,
            effective,
            tls: route.tls,
            service: route.service.clone(),
            origin,
        });
    }
    if let (Some(declared_base), Some(effective_base)) =
        (declared_base.as_deref(), effective_base.as_deref())
    {
        for alias in service_aliases {
            let declared = format!("{}.{}", alias.domain_label, declared_base);
            let effective = format!("{}.{}", alias.domain_label, effective_base);
            if !occupied.insert(declared.clone()) {
                continue;
            }
            occupied.insert(effective.clone());
            routes.push(EffectiveHostRoute {
                kind: HostRouteKind::Tcp,
                declared,
                effective,
                tls: false,
                service: Some(alias.service.clone()),
                origin: None,
            });
        }
        for shared in shared_services {
            let declared = format!("{}.{}", shared.domain_label, declared_base);
            let effective = format!("{}.{}", shared.domain_label, effective_base);
            if !occupied.insert(declared.clone()) {
                continue;
            }
            occupied.insert(effective.clone());
            routes.push(EffectiveHostRoute {
                kind: HostRouteKind::Tcp,
                declared,
                effective,
                tls: false,
                service: Some(shared.service_name.clone()),
                origin: None,
            });
        }
    }
    EffectiveHostMap {
        kind: if token.is_some() {
            HostScopeKind::Worktree
        } else {
            HostScopeKind::Primary
        },
        token: token.map(str::to_owned),
        host_key: rewrite_key.map(str::to_owned).or_else(|| {
            token
                .filter(|_| shared_runtime_identity)
                .and_then(host_key)
                .map(str::to_owned)
        }),
        shared_runtime_identity,
        declared_base,
        effective_base,
        routes,
    }
}

pub fn scope_token(
    repo_root: &std::path::Path,
    share_runtime_identity: bool,
) -> Result<Option<String>, super::model::ContainerPolicyError> {
    if share_runtime_identity {
        return Ok(None);
    }
    worktree_scope::load_or_create(repo_root).map_err(|error| {
        super::model::ContainerPolicyError::TaskInvocation(format!(
            "cannot establish checkout runtime scope for {}: {error}",
            repo_root.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(domain: &str, tls: bool, service: Option<&str>) -> EffectiveDnsRoute {
        EffectiveDnsRoute {
            domain: domain.to_owned(),
            declared_domain: domain.to_owned(),
            tls,
            port: Some(80),
            service: service.map(str::to_owned),
            target_host: None,
        }
    }

    #[test]
    fn primary_checkout_keeps_declared_hosts() {
        let map = build_host_map(
            &[
                http("acowtancy.test", true, Some("web")),
                http("admin.acowtancy.test", true, Some("admin")),
            ],
            &[EffectiveServiceAlias {
                service: "db".to_owned(),
                domain_label: "postgres".to_owned(),
                container_port: 5432,
            }],
            &[],
            None,
            false,
        );
        assert_eq!(map.kind, HostScopeKind::Primary);
        assert_eq!(map.effective_base.as_deref(), Some("acowtancy.test"));
        assert_eq!(map.cookie_domain(), Some("acowtancy.test"));
        assert_eq!(map.routes[0].effective, "acowtancy.test");
        assert_eq!(
            map.routes[0].origin.as_deref(),
            Some("https://acowtancy.test")
        );
        assert_eq!(map.routes[1].effective, "admin.acowtancy.test");
        assert_eq!(map.routes[2].kind, HostRouteKind::Tcp);
        assert_eq!(map.routes[2].effective, "postgres.acowtancy.test");
    }

    #[test]
    fn worktree_rewrites_apex_for_http_and_tcp_together() {
        let token = "abcdef0123456789abcdef0123456789";
        let map = build_host_map(
            &[
                http("acowtancy.test", true, Some("web")),
                http("mail.acowtancy.test", false, Some("mailpit")),
            ],
            &[EffectiveServiceAlias {
                service: "db".to_owned(),
                domain_label: "postgres".to_owned(),
                container_port: 5432,
            }],
            &[SharedServiceBinding {
                service_name: "cache".to_owned(),
                catalog: "redis".to_owned(),
                domain_label: "redis".to_owned(),
                project_name: "shared-redis".to_owned(),
                compose_file: std::path::PathBuf::from("compose.yml"),
                host: "127.0.0.1".to_owned(),
                host_port: 6379,
                container_port: 6379,
                host_env_vars: vec![],
                port_env_vars: vec![],
            }],
            Some(token),
            false,
        );
        assert_eq!(map.kind, HostScopeKind::Worktree);
        assert_eq!(map.host_key.as_deref(), Some("abcdef01"));
        assert_eq!(
            map.effective_base.as_deref(),
            Some("acowtancy-wabcdef01.test")
        );
        assert_eq!(map.routes[0].declared, "acowtancy.test");
        assert_eq!(map.routes[0].effective, "acowtancy-wabcdef01.test");
        assert_eq!(
            map.routes[0].origin.as_deref(),
            Some("https://acowtancy-wabcdef01.test")
        );
        assert_eq!(map.routes[1].effective, "mail.acowtancy-wabcdef01.test");
        assert_eq!(
            map.routes[1].origin.as_deref(),
            Some("http://mail.acowtancy-wabcdef01.test")
        );
        let tcp = map
            .routes
            .iter()
            .filter(|route| route.kind == HostRouteKind::Tcp)
            .collect::<Vec<_>>();
        assert_eq!(tcp[0].effective, "postgres.acowtancy-wabcdef01.test");
        assert_eq!(tcp[1].effective, "redis.acowtancy-wabcdef01.test");
        assert_ne!(map.effective_base.as_deref(), map.declared_base.as_deref());
    }

    #[test]
    fn sibling_tokens_produce_distinct_hostnames() {
        let first = build_host_map(
            &[http("app.test", false, None)],
            &[],
            &[],
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            false,
        );
        let second = build_host_map(
            &[http("app.test", false, None)],
            &[],
            &[],
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            false,
        );
        assert_eq!(first.routes[0].effective, "app-waaaaaaaa.test");
        assert_eq!(second.routes[0].effective, "app-wbbbbbbbb.test");
        assert_ne!(first.routes[0].effective, second.routes[0].effective);
    }

    #[test]
    fn shared_runtime_identity_keeps_declared_hosts() {
        let map = build_host_map(
            &[http("app.test", true, None)],
            &[],
            &[],
            Some("cccccccccccccccccccccccccccccccc"),
            true,
        );
        assert_eq!(map.effective_base.as_deref(), Some("app.test"));
        assert_eq!(map.routes[0].effective, "app.test");
        assert!(map.shared_runtime_identity);
    }

    #[test]
    fn scope_kind_override_relabels_clone_without_touching_routes() {
        let token = "abcdef0123456789abcdef0123456789";
        let map = build_host_map(
            &[http("app.test", false, None)],
            &[],
            &[],
            Some(token),
            false,
        )
        .with_scope_kind(HostScopeKind::EphemeralClone);
        assert_eq!(map.kind, HostScopeKind::EphemeralClone);
        assert_eq!(map.routes[0].effective, "app-wabcdef01.test");
        assert_eq!(map.host_key.as_deref(), Some("abcdef01"));
    }
}
