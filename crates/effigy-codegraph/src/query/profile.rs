use std::collections::BTreeSet;

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
    pub(super) fn new(request: &str) -> Self {
        let raw_tokens = request
            .split_whitespace()
            .flat_map(split_identifier_token)
            .map(|token| token.to_ascii_lowercase())
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        let intent = classify_request_intent(&raw_tokens);
        let match_tokens = raw_tokens
            .iter()
            .filter(|token| !is_context_stop_word(token))
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
    let mut tokens = vec![token.to_owned()];
    match token {
        "change" | "changes" | "changed" => {
            tokens.extend(
                ["change", "changes", "changed"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        "detect" | "detection" | "detected" => {
            tokens.extend(
                ["detect", "detection", "scan"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        "stale" | "staleness" => {
            tokens.extend(
                ["stale", "staleness", "freshness"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        "route" | "routes" | "routing" | "routed" => {
            tokens.extend(
                ["route", "routes", "routing", "selector", "selectors"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        "parse" | "parsed" | "parser" | "parsing" => {
            tokens.extend(
                ["parse", "parsed", "parsing"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        _ => {}
    }
    tokens
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
