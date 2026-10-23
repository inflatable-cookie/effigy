//! Effective gateway hostnames for one runtime scope.
//!
//! The primary checkout keeps declared domains. A linked worktree or a marked
//! ephemeral clone that does not share runtime identity rewrites the apex so
//! every declared HTTP route and TCP alias stays together under
//! `<apex>-w<host-key>.<tld>`.

use super::model::{
    EffectiveContainerPolicy, EffectiveDnsRoute, EffectiveHostProcess, EffectiveServiceAlias,
    SharedServiceBinding,
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
    /// True when no `base_domain` was declared and this map inferred one from
    /// the HTTP routes. A declared value is never inferred or rewritten.
    pub base_domain_inferred: bool,
    /// Why an inferred base cannot be a suffix of every HTTP route. `None`
    /// when the base was declared or the routes agree.
    pub base_domain_warning: Option<String>,
    pub routes: Vec<EffectiveHostRoute>,
}

/// The candidate base domain for a checkout that declares none, plus the
/// operator-facing warning when the routes disagree about their apex.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InferredBaseDomain {
    pub base: Option<String>,
    pub warning: Option<String>,
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

/// Split a domain into its non-empty labels.
fn domain_labels(domain: &str) -> Vec<&str> {
    domain
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn has_usable_base(domain: &str) -> bool {
    domain_labels(domain).len() >= 2
}

/// Whether `domain` ends with the labels of `base`.
pub fn domain_has_suffix(domain: &str, base: &str) -> bool {
    let domain = domain_labels(domain);
    let base = domain_labels(base);
    !base.is_empty()
        && domain.len() >= base.len()
        && domain[domain.len() - base.len()..]
            .iter()
            .zip(&base)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

/// The candidate HTTP route domains an undeclared base is inferred from: the
/// declared DNS routes, front first, then the managed listener routes.
fn base_domain_candidates(
    dns_routes: &[EffectiveDnsRoute],
    listener_declared_domains: &[String],
) -> Vec<String> {
    let mut candidates = dns_routes
        .iter()
        .map(|route| route.declared().to_owned())
        .collect::<Vec<_>>();
    candidates.extend(listener_declared_domains.iter().cloned());
    candidates
}

/// Resolve the declared-space apex that generated TCP aliases and gateway
/// registration use: the declared `base_domain` untouched, else the inferred
/// common suffix of the HTTP routes and managed listeners. Returning `None`
/// means the checkout has no usable apex, never that it falls back to the
/// front route's full domain.
pub fn resolve_base_domain(
    dns_routes: &[EffectiveDnsRoute],
    listener_declared_domains: &[String],
    base_domain: Option<&str>,
) -> Option<String> {
    match base_domain {
        Some(declared) => {
            let declared = declared.trim();
            has_usable_base(declared).then(|| declared.to_owned())
        }
        None => infer_base_domain(&base_domain_candidates(
            dns_routes,
            listener_declared_domains,
        ))
        .base
        .filter(|domain| has_usable_base(domain)),
    }
}

/// The host key a checkout's declared routes were rewritten with, if any.
/// Scoped routes carry the declared/effective label pair
/// `acme.test` -> `acme-w<key>.test`; nothing else is evidence of a rewrite.
pub fn route_scope_host_key(dns_routes: &[EffectiveDnsRoute]) -> Option<String> {
    for route in dns_routes {
        let declared = domain_labels(route.declared());
        let effective = domain_labels(route.domain.trim());
        if declared == effective || declared.len() < 2 || declared.len() != effective.len() {
            continue;
        }
        let declared_apex = declared[declared.len() - 2];
        let effective_apex = effective[effective.len() - 2];
        if let Some(key) = effective_apex
            .strip_prefix(declared_apex)
            .and_then(|rest| rest.strip_prefix("-w"))
            .filter(|key| !key.is_empty())
        {
            return Some(key.to_owned());
        }
    }
    None
}

impl EffectiveContainerPolicy {
    /// The apex generated TCP aliases and gateway registration use for this
    /// checkout: `dns.base_domain` when declared (rewritten with the checkout
    /// host key exactly like the routes), else the inferred common suffix of
    /// the HTTP routes and managed listeners. `container hosts` reports the
    /// same apex, so the gateway, Compose aliases and host map never disagree.
    pub fn dns_effective_base_domain(&self) -> Option<String> {
        let listener_declared_domains = self
            .host_processes
            .iter()
            .filter_map(|process| process.listener.as_ref())
            .map(|listener| listener.declared_route_domain.clone())
            .collect::<Vec<_>>();
        let declared = match resolve_base_domain(
            &self.dns_routes,
            &listener_declared_domains,
            self.dns_base_domain.as_deref(),
        ) {
            Some(declared) => declared,
            // A hand-built policy may declare only the primary domain. Real
            // policies always carry the route it came from, and a route list
            // is never silently replaced by the first route's full domain.
            None if self.dns_routes.is_empty() && listener_declared_domains.is_empty() => {
                self.dns_domain.as_deref()?.trim().to_owned()
            }
            None => return None,
        };
        if !has_usable_base(&declared) {
            return None;
        }
        Some(match route_scope_host_key(&self.dns_routes) {
            Some(key) => rewrite_declared_host(&declared, &key),
            None => declared,
        })
    }
}

/// Infer an undeclared base domain from the longest common domain suffix of
/// the HTTP routes. The front route anchors the answer, which keeps a bundle
/// that declares a subdomain first (`s3.acme.test`) from naming that subdomain
/// as its cookie apex, RP ID and TCP alias suffix. When the routes share only a
/// top-level label, the front route's own apex is used and the routes that
/// disagree are reported so the operator can declare `base_domain`.
pub fn infer_base_domain(http_domains: &[String]) -> InferredBaseDomain {
    let routes = http_domains
        .iter()
        .map(|domain| domain_labels(domain))
        .filter(|labels| !labels.is_empty())
        .collect::<Vec<_>>();
    let Some(front) = routes.first() else {
        return InferredBaseDomain::default();
    };
    let mut common = front.len();
    for labels in &routes[1..] {
        let mut matching = 0;
        while matching < common
            && matching < labels.len()
            && front[front.len() - 1 - matching]
                .eq_ignore_ascii_case(labels[labels.len() - 1 - matching])
        {
            matching += 1;
        }
        common = matching;
    }
    // A single shared top-level label would make every cookie domain the bare
    // TLD, so prefer the front route's apex and report the disagreement.
    let base = if common >= 2 {
        front[front.len() - common..].join(".")
    } else {
        front[front.len().saturating_sub(2)..].join(".")
    };
    let inconsistent = if common >= 2 {
        Vec::new()
    } else {
        http_domains
            .iter()
            .filter(|domain| !domain_has_suffix(domain, &base))
            .map(|domain| format!("`{domain}`"))
            .collect::<Vec<_>>()
    };
    let warning = (!inconsistent.is_empty()).then(|| {
        format!(
            "inferred base domain `{base}` is not a suffix of every HTTP route ({}); declare `[containers.<name>.dns] base_domain` to choose the cookie domain, RP ID and TCP alias apex",
            inconsistent.join(", ")
        )
    });
    InferredBaseDomain {
        base: Some(base),
        warning,
    }
}

pub fn build_host_map(
    dns_routes: &[EffectiveDnsRoute],
    service_aliases: &[EffectiveServiceAlias],
    shared_services: &[SharedServiceBinding],
    token: Option<&str>,
    shared_runtime_identity: bool,
    base_domain: Option<&str>,
) -> EffectiveHostMap {
    build_host_map_including_listeners(
        dns_routes,
        service_aliases,
        shared_services,
        token,
        shared_runtime_identity,
        base_domain,
        &[],
    )
}

/// Like [`build_host_map`], with the checkout's managed host-listener routes
/// contributing their declared domains as inference candidates. Listener
/// routes are appended by [`add_managed_listener_routes`] after this map is
/// built; naming them here lets an undeclared base follow every HTTP route,
/// and lets a managed listener alone establish the apex.
#[allow(clippy::too_many_arguments)]
pub fn build_host_map_including_listeners(
    dns_routes: &[EffectiveDnsRoute],
    service_aliases: &[EffectiveServiceAlias],
    shared_services: &[SharedServiceBinding],
    token: Option<&str>,
    shared_runtime_identity: bool,
    base_domain: Option<&str>,
    listener_declared_domains: &[String],
) -> EffectiveHostMap {
    let rewrite_key = token
        .filter(|_| !shared_runtime_identity)
        .and_then(host_key);
    let inferred = base_domain.is_none().then(|| {
        infer_base_domain(&base_domain_candidates(
            dns_routes,
            listener_declared_domains,
        ))
    });
    let declared_base = resolve_base_domain(dns_routes, listener_declared_domains, base_domain);
    let base_domain_inferred = base_domain.is_none() && declared_base.is_some();
    let base_domain_warning = inferred
        .and_then(|inferred| inferred.warning)
        .filter(|_| base_domain_inferred);
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
        base_domain_inferred,
        base_domain_warning,
        routes,
    }
}

/// Append managed host-listener declarations to the DNS inventory. Manifest
/// validation already refuses a listener name that matches a static DNS route,
/// so each listener adds one entry with its own declared-to-effective pair.
pub fn add_managed_listener_routes(
    map: &mut EffectiveHostMap,
    host_processes: &[EffectiveHostProcess],
) {
    for listener in host_processes
        .iter()
        .filter_map(|process| process.listener.as_ref())
    {
        let scheme = if listener.route_tls { "https" } else { "http" };
        map.routes.push(EffectiveHostRoute {
            kind: HostRouteKind::Http,
            declared: listener.declared_route_domain.clone(),
            effective: listener.route_domain.clone(),
            tls: listener.route_tls,
            service: None,
            origin: Some(format!("{scheme}://{}", listener.route_domain)),
        });
    }
}

pub fn scope_token(
    repo_root: &std::path::Path,
    share_runtime_identity: bool,
) -> Result<Option<String>, super::model::ContainerPolicyError> {
    if share_runtime_identity {
        return Ok(None);
    }
    worktree_scope::load(repo_root).map_err(|error| {
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
            None,
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
    fn explicit_base_domain_sets_cookie_and_tcp_alias_apex() {
        let token = "abcdef0123456789abcdef0123456789";
        let map = build_host_map(
            &[http("s3.acme.test", true, Some("web"))],
            &[EffectiveServiceAlias {
                service: "db".to_owned(),
                domain_label: "postgres".to_owned(),
                container_port: 5432,
            }],
            &[],
            Some(token),
            false,
            Some("acme.test"),
        );

        assert_eq!(map.declared_base.as_deref(), Some("acme.test"));
        assert_eq!(map.cookie_domain(), Some("acme-wabcdef01.test"));
        assert_eq!(map.routes[0].effective, "s3.acme-wabcdef01.test");
        assert_eq!(map.routes[1].declared, "postgres.acme.test");
        assert_eq!(map.routes[1].effective, "postgres.acme-wabcdef01.test");
    }

    #[test]
    fn cookie_and_tcp_alias_apex_default_to_first_route_domain() {
        let map = build_host_map(
            &[http("s3.acme.test", true, Some("web"))],
            &[EffectiveServiceAlias {
                service: "db".to_owned(),
                domain_label: "postgres".to_owned(),
                container_port: 5432,
            }],
            &[],
            None,
            false,
            None,
        );

        assert_eq!(map.cookie_domain(), Some("s3.acme.test"));
        assert_eq!(map.routes[1].effective, "postgres.s3.acme.test");
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
            None,
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
            None,
        );
        let second = build_host_map(
            &[http("app.test", false, None)],
            &[],
            &[],
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            false,
            None,
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
            None,
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
            None,
        )
        .with_scope_kind(HostScopeKind::EphemeralClone);
        assert_eq!(map.kind, HostScopeKind::EphemeralClone);
        assert_eq!(map.routes[0].effective, "app-wabcdef01.test");
        assert_eq!(map.host_key.as_deref(), Some("abcdef01"));
    }

    fn alias() -> EffectiveServiceAlias {
        EffectiveServiceAlias {
            service: "db".to_owned(),
            domain_label: "postgres".to_owned(),
            container_port: 5432,
        }
    }

    #[test]
    fn undeclared_base_infers_the_shared_suffix_not_the_front_subdomain() {
        let map = build_host_map(
            &[
                http("s3.acowtancy.test", true, Some("web")),
                http("app.s3.acowtancy.test", true, Some("app")),
                http("admin.acowtancy.test", true, Some("admin")),
            ],
            &[alias()],
            &[],
            None,
            false,
            None,
        );
        assert!(map.base_domain_inferred);
        assert_eq!(map.base_domain_warning, None);
        assert_eq!(map.declared_base.as_deref(), Some("acowtancy.test"));
        assert_eq!(map.cookie_domain(), Some("acowtancy.test"));
        assert_eq!(map.relying_party_id(), Some("acowtancy.test"));
        assert_eq!(map.routes[3].declared, "postgres.acowtancy.test");
    }

    #[test]
    fn declared_base_is_never_inferred_or_rewritten() {
        let map = build_host_map(
            &[
                http("s3.acme.test", true, Some("web")),
                http("acme.test", true, Some("site")),
            ],
            &[],
            &[],
            None,
            false,
            Some("declared.test"),
        );
        assert!(!map.base_domain_inferred);
        assert_eq!(map.base_domain_warning, None);
        assert_eq!(map.declared_base.as_deref(), Some("declared.test"));
        assert_eq!(map.cookie_domain(), Some("declared.test"));
    }

    #[test]
    fn routes_without_a_shared_apex_infer_the_front_and_warn() {
        let map = build_host_map(
            &[
                http("s3.acme.test", true, Some("web")),
                http("other.example", true, Some("other")),
            ],
            &[alias()],
            &[],
            None,
            false,
            None,
        );
        assert!(map.base_domain_inferred);
        assert_eq!(map.declared_base.as_deref(), Some("acme.test"));
        let warning = map.base_domain_warning.as_deref().expect("a warning");
        assert!(
            warning.contains("not a suffix of every HTTP route"),
            "{warning}"
        );
        assert!(warning.contains("other.example"), "{warning}");
        assert!(warning.contains("base_domain"), "{warning}");
    }

    #[test]
    fn managed_listener_routes_join_base_domain_inference() {
        let map = build_host_map_including_listeners(
            &[http("s3.acme.test", true, Some("web"))],
            &[alias()],
            &[],
            None,
            false,
            None,
            &["cbs.acme.test".to_owned()],
        );
        assert!(map.base_domain_inferred);
        assert_eq!(map.base_domain_warning, None);
        assert_eq!(map.declared_base.as_deref(), Some("acme.test"));
        assert_eq!(map.routes[1].declared, "postgres.acme.test");

        // Without the listener hint a lone subdomain route is all Effigy can
        // see, which is why bundles are told to declare `base_domain`.
        let without_listener = build_host_map(
            &[http("s3.acme.test", true, Some("web"))],
            &[],
            &[],
            None,
            false,
            None,
        );
        assert_eq!(
            without_listener.declared_base.as_deref(),
            Some("s3.acme.test")
        );
    }

    #[test]
    fn domain_suffix_matching_is_label_aware() {
        assert!(domain_has_suffix("s3.acme.test", "acme.test"));
        assert!(domain_has_suffix("acme.test", "acme.test"));
        assert!(!domain_has_suffix("evilacme.test", "acme.test"));
        assert!(!domain_has_suffix("acme.test", "s3.acme.test"));
    }

    fn scoped_route(domain: &str, declared: &str) -> EffectiveDnsRoute {
        let mut route = http(declared, true, None);
        route.domain = domain.to_owned();
        route
    }

    #[test]
    fn resolved_base_domain_prefers_declaration_and_infers_otherwise() {
        let routes = [
            http("s3.acme.test", true, None),
            http("admin.acme.test", true, None),
        ];
        assert_eq!(
            resolve_base_domain(&routes, &[], Some("declared.test")).as_deref(),
            Some("declared.test"),
            "a declared apex is kept untouched"
        );
        assert_eq!(
            resolve_base_domain(&routes, &[], None).as_deref(),
            Some("acme.test"),
            "an undeclared apex is the routes' common suffix"
        );
        assert_eq!(
            resolve_base_domain(&routes, &["cbs.other.test".to_owned()], None).as_deref(),
            Some("acme.test"),
            "a managed listener that disagrees keeps the front route's apex"
        );
    }

    #[test]
    fn route_scope_host_key_only_reads_a_rewritten_route() {
        let unscoped = [http("s3.acme.test", true, None)];
        assert_eq!(route_scope_host_key(&unscoped), None);
        let scoped = [scoped_route("s3.acme-wabcdef01.test", "s3.acme.test")];
        assert_eq!(route_scope_host_key(&scoped).as_deref(), Some("abcdef01"));
    }

    #[test]
    fn policy_effective_base_domain_matches_the_host_map_apex() {
        let fixture = tempfile::tempdir().expect("private checkout");
        let mut policy = crate::policy::model::test_policy(fixture.path());
        policy.dns_routes = vec![
            scoped_route("s3.acme-wabcdef01.test", "s3.acme.test"),
            scoped_route("admin.acme-wabcdef01.test", "admin.acme.test"),
        ];
        assert_eq!(
            policy.dns_effective_base_domain().as_deref(),
            Some("acme-wabcdef01.test")
        );

        policy.dns_base_domain = Some("acme.test".to_owned());
        assert_eq!(
            policy.dns_effective_base_domain().as_deref(),
            Some("acme-wabcdef01.test"),
            "a declared apex is rewritten exactly like the routes"
        );

        let map = build_host_map(
            &policy.dns_routes,
            &[],
            &[],
            Some("abcdef0123456789abcdef0123456789"),
            false,
            policy.dns_base_domain.as_deref(),
        );
        assert_eq!(
            policy.dns_effective_base_domain(),
            map.effective_base,
            "gateway and Compose aliases share the host map's apex"
        );
    }
}
