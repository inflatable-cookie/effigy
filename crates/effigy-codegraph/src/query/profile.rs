use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FileRole {
    Implementation,
    Config,
    Test,
    Docs,
    Planning,
    Fixture,
    Generated,
}

impl FileRole {
    pub(super) fn classify(path: &str, language_id: &str) -> Self {
        let lower = path.to_ascii_lowercase();
        if lower.contains("/target/")
            || lower.contains("/node_modules/")
            || lower.contains("/vendor/")
            || lower.contains("/.effigy/")
        {
            return Self::Generated;
        }
        if lower.contains("/fixtures/")
            || lower.contains("/fixture/")
            || lower.contains("/examples/")
            || lower.starts_with("examples/")
        {
            return Self::Fixture;
        }
        if lower.starts_with("tests/")
            || lower.contains("/tests/")
            || lower.ends_with("/tests.rs")
            || lower.ends_with("_test.rs")
            || lower.ends_with("_tests.rs")
            || lower.ends_with(".test.ts")
            || lower.ends_with(".spec.ts")
            || lower.ends_with(".test.js")
            || lower.ends_with(".spec.js")
        {
            return Self::Test;
        }
        if lower.starts_with("config/")
            || lower.ends_with(".toml")
            || lower.ends_with(".json")
            || lower.ends_with(".yaml")
            || lower.ends_with(".yml")
        {
            return Self::Config;
        }
        if lower.starts_with("docs/roadmaps/")
            || lower.starts_with("docs/specs/")
            || lower.starts_with("docs/logs/")
        {
            return Self::Planning;
        }
        if language_id == "markdown" || lower.starts_with("docs/") || lower.ends_with(".md") {
            return Self::Docs;
        }
        Self::Implementation
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Implementation => "implementation",
            Self::Config => "config",
            Self::Test => "test",
            Self::Docs => "docs",
            Self::Planning => "planning",
            Self::Fixture => "fixture",
            Self::Generated => "generated",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RequestIntent {
    Implementation,
    Test,
    Docs,
    General,
}

#[derive(Debug, Clone)]
pub(super) struct RequestProfile {
    pub(super) normalized_request: String,
    pub(super) match_tokens: Vec<String>,
    pub(super) intent: RequestIntent,
}

impl RequestProfile {
    pub(super) fn new(request: &str, repo_root: &Path) -> Self {
        let raw_tokens = request
            .split_whitespace()
            .flat_map(split_identifier_token)
            .map(|token| token.to_ascii_lowercase())
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        let intent = classify_request_intent(&raw_tokens);
        let repo_tokens = repo_identity_tokens(repo_root);
        let match_tokens = raw_tokens
            .iter()
            .filter(|token| !is_context_stop_word(token))
            .filter(|token| !repo_tokens.contains(*token))
            .flat_map(|token| expanded_match_tokens(token))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        Self {
            normalized_request: match_tokens.join(" "),
            match_tokens,
            intent,
        }
    }

    pub(super) fn prefers_crate_root(&self) -> bool {
        self.match_tokens
            .iter()
            .any(|token| matches!(token.as_str(), "orchestration" | "architecture"))
    }

    pub(super) fn role_adjustment(&self, role: FileRole) -> i64 {
        match (self.intent, role) {
            (RequestIntent::Implementation, FileRole::Implementation) => 6,
            (RequestIntent::Implementation, FileRole::Config) => -2,
            (RequestIntent::Implementation, FileRole::Test) => -5,
            (RequestIntent::Implementation, FileRole::Docs | FileRole::Planning) => -8,
            (RequestIntent::Implementation, FileRole::Fixture) => -3,
            (RequestIntent::Implementation, FileRole::Generated) => -8,
            (RequestIntent::Test, FileRole::Test) => 6,
            (RequestIntent::Test, FileRole::Implementation) => 2,
            (RequestIntent::Test, FileRole::Docs | FileRole::Planning) => -2,
            (RequestIntent::Docs, FileRole::Docs) => 7,
            (RequestIntent::Docs, FileRole::Planning) => 4,
            (RequestIntent::Docs, FileRole::Implementation) => -2,
            (RequestIntent::Docs, FileRole::Config) => -1,
            (RequestIntent::Docs, FileRole::Test) => -3,
            (RequestIntent::General, FileRole::Generated) => -6,
            (RequestIntent::General, FileRole::Docs | FileRole::Planning) => -5,
            (RequestIntent::General, FileRole::Config) => -2,
            (RequestIntent::General, FileRole::Implementation) => 3,
            _ => 0,
        }
    }
}

fn classify_request_intent(tokens: &[String]) -> RequestIntent {
    if tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "test" | "tests" | "regression" | "fixture" | "fixtures" | "coverage"
        )
    }) {
        return RequestIntent::Test;
    }
    if tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "doc"
                | "docs"
                | "guide"
                | "guides"
                | "contract"
                | "contracts"
                | "roadmap"
                | "roadmaps"
                | "skill"
                | "skills"
        )
    }) {
        return RequestIntent::Docs;
    }
    if tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "trace"
                | "implement"
                | "implementation"
                | "owner"
                | "runtime"
                | "command"
                | "flow"
                | "find"
                | "where"
                | "how"
                | "change"
                | "changes"
                | "understand"
                | "resolve"
                | "resolution"
        )
    }) {
        return RequestIntent::Implementation;
    }
    RequestIntent::General
}

fn is_context_stop_word(token: &str) -> bool {
    matches!(
        token,
        "trace"
            | "find"
            | "where"
            | "how"
            | "what"
            | "when"
            | "why"
            | "understand"
            | "implementation"
            | "implement"
            | "owner"
            | "flow"
            | "does"
            | "do"
            | "did"
            | "the"
            | "this"
            | "that"
            | "a"
            | "an"
            | "and"
            | "or"
            | "for"
            | "to"
            | "of"
            | "in"
            | "with"
            | "by"
            | "from"
            | "on"
            | "at"
            | "as"
            | "if"
            | "then"
            | "is"
            | "are"
    )
}

fn expanded_match_tokens(token: &str) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    insert_token_family(&mut tokens, token);
    match token {
        "change" | "changes" | "changed" => {
            for variant in ["change", "changes", "changed"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "detect" | "detection" | "detected" => {
            for variant in ["detect", "detection", "scan"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "stale" | "staleness" => {
            for variant in ["stale", "staleness", "freshness", "refresh"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "route" | "routes" | "routing" | "routed" => {
            for variant in ["route", "routes", "routing", "selector", "selectors"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "parse" | "parsed" | "parser" | "parsing" => {
            for variant in ["parse", "parsed", "parsing"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "prompt" | "prompts" | "prompted" | "confirm" | "confirms" | "confirmation"
        | "confirming" | "ask" | "asks" | "interactive" => {
            for variant in ["prompt", "confirm", "confirmation", "ask", "interactive"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "shut" | "shutdown" | "stop" | "stops" | "teardown" | "closeout" | "cleanup" | "close"
        | "down" => {
            for variant in [
                "shutdown", "stop", "teardown", "closeout", "cleanup", "close",
            ] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "exit" | "exits" | "exiting" => {
            for variant in ["exit", "closeout", "cleanup", "teardown"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "validate" | "validation" | "verify" | "verified" | "check" | "checks" | "guard" => {
            for variant in ["validate", "validation", "verify", "check", "guard"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "redirect" | "redirects" | "rewrite" | "rewrites" | "forward" | "forwards" => {
            for variant in ["redirect", "rewrite", "forward"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "migrate" | "migrates" | "migration" | "upgrade" | "upgrades" | "convert" | "converts"
        | "adopt" | "adopts" => {
            for variant in ["migrate", "migration", "upgrade", "convert", "adopt"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "cache" | "caches" | "cached" | "caching" => {
            for variant in ["cache", "cached", "caching"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        "index" | "indexes" | "indexed" | "indexing" => {
            for variant in ["index", "indexed", "indexing", "freshness", "refresh"] {
                insert_token_family(&mut tokens, variant);
            }
        }
        _ => {}
    }
    tokens.into_iter().collect()
}

fn repo_identity_tokens(repo_root: &Path) -> BTreeSet<String> {
    repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .map(split_identifier_token)
        .unwrap_or_default()
        .into_iter()
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn insert_token_family(tokens: &mut BTreeSet<String>, token: &str) {
    if token.is_empty() {
        return;
    }
    tokens.insert(token.to_owned());
    if let Some(singular) = singularize_token(token) {
        tokens.insert(singular);
    }
}

fn singularize_token(token: &str) -> Option<String> {
    if token.len() <= 3 {
        return None;
    }
    if let Some(stem) = token.strip_suffix("ies") {
        return (!stem.is_empty()).then(|| format!("{stem}y"));
    }
    if token.ends_with('s') && !token.ends_with("ss") {
        return token.strip_suffix('s').map(str::to_owned);
    }
    None
}

pub(super) fn split_identifier_token(token: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let lowered = token.to_ascii_lowercase();
    let route_token = lowered
        .trim_matches(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '"' | '\'' | ',' | '.' | '?' | '!' | '(' | ')' | '[' | ']'
                )
        })
        .to_owned();
    if route_token.contains('/') && route_token.chars().any(|ch| ch.is_ascii_alphanumeric()) {
        tokens.push(route_token.clone());
        for segment in route_token.split('/') {
            let cleaned = segment.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
            if !cleaned.is_empty() {
                tokens.push(cleaned.to_owned());
            }
        }
    }
    let cleaned = token
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .replace(['_', '-'], " ");
    for part in cleaned.split_whitespace() {
        let mut current = String::new();
        for ch in part.chars() {
            if ch.is_ascii_uppercase() && !current.is_empty() {
                tokens.push(current.to_ascii_lowercase());
                current.clear();
            }
            current.push(ch);
        }
        if !current.is_empty() {
            tokens.push(current.to_ascii_lowercase());
        }
    }
    tokens.sort();
    tokens.dedup();
    tokens
}
