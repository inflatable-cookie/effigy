//! Targeted usage for an argument error (effigy#318).
//!
//! An argument error names *what* is wrong; this module supplies the *where*.
//! It walks the typed words against the nearest command's own help panel and
//! returns the words that command accepts at the point the typed word failed,
//! the panel's `Usage` lines when there is nothing closer, and a supported
//! form when the typed word is a near miss or a command that lives elsewhere.
//! The full help panel is never part of an argument error.
//!
//! The panel is the single owner of the shapes: nothing here restates a
//! command's grammar, so a usage line that changes changes this answer too.

use crate::help::{render_help, HelpRenderer, HelpResult, KeyValue, NoticeLevel, TableSpec};
use crate::HelpTopic;

/// The targeted answer to one argument error.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsageHint {
    /// The typed prefix that resolved to a real command, e.g. `effigy container`.
    pub command: String,
    /// One or two usage lines of the nearest resolved command.
    pub usage: Vec<String>,
    /// Words the nearest command accepts where the typed word failed.
    pub supported: Vec<String>,
    /// A supported form the typed word most likely meant.
    pub did_you_mean: Option<String>,
}

impl UsageHint {
    /// The one line that points at the rest of the command's own help.
    pub fn help_hint(&self) -> String {
        if self.command.is_empty() {
            "Run `effigy --help` to see supported command forms".to_owned()
        } else {
            format!("Run `{} --help` for every form", self.command)
        }
    }

    /// Whether this hint says anything more than "read the help".
    pub fn is_empty(&self) -> bool {
        self.usage.is_empty() && self.supported.is_empty() && self.did_you_mean.is_none()
    }
}

/// The targeted answer for an argument error whose argv is `argv`.
///
/// `argv` is the command argv after global flags are stripped, exactly as the
/// parser saw it. An empty or unplaceable argv yields a hint that only points
/// at `effigy --help`.
pub fn usage_hint_for(argv: &[String]) -> UsageHint {
    // A leading global flag is never the command word.
    let Some(first) = argv.iter().find(|arg| !arg.starts_with('-')) else {
        return UsageHint::default();
    };
    let Some(topic) = topic_for_word(first) else {
        return UsageHint::default();
    };
    let panel = Panel::of(topic);
    let (words, flags) = split_argv(argv);
    let failures = collect_failures(&panel.walk(&words), &words);
    if failures.is_empty() {
        // Every typed word resolved: the error is a flag or a value, so the
        // answer is the nearest command's own usage lines.
        return UsageHint {
            command: command_path(&words, words.len()),
            usage: panel.nearest_usage(&words, words.len()),
            supported: Vec::new(),
            did_you_mean: unknown_flag(&panel, &flags),
        };
    }
    // The earliest place the typed words stop matching a real command is where
    // a person needs to read. An earlier word can fail only because a later one
    // is wrong (`container exec app`: `exec` is not a `container` action at
    // all), so the first failure that has an answer wins.
    for failure in &failures {
        if let Some(answer) = answer_for(&panel, &words, failure) {
            return UsageHint {
                command: answer.command,
                usage: answer.usage,
                supported: answer.supported,
                did_you_mean: answer.did_you_mean,
            };
        }
    }
    let failure = &failures[failures.len() - 1];
    UsageHint {
        command: command_path(&words, failure.position),
        usage: Vec::new(),
        supported: failure.supported.clone(),
        did_you_mean: unknown_flag(&panel, &flags),
    }
}

/// Every usage line the panel of `topic` renders, in render order.
pub fn usage_lines(topic: HelpTopic) -> Vec<String> {
    Panel::of(topic).usage
}

/// Every option spelling the panel of `topic` renders, in render order.
pub fn option_lines(topic: HelpTopic) -> Vec<String> {
    Panel::of(topic).options
}

/// One supported form for a failure.
#[derive(Debug, Clone, Default)]
struct Answer {
    command: String,
    supported: Vec<String>,
    usage: Vec<String>,
    did_you_mean: Option<String>,
}

#[derive(Debug, Clone)]
struct Failure {
    position: usize,
    word: String,
    supported: Vec<String>,
}

fn collect_failures(walks: &[Walk], words: &[String]) -> Vec<Failure> {
    let mut failures: Vec<Failure> = Vec::new();
    for walk in walks {
        let Some(position) = walk.failure else {
            continue;
        };
        let Some(word) = words.get(position) else {
            continue;
        };
        match failures.iter_mut().find(|entry| entry.position == position) {
            Some(entry) => {
                for supported in &walk.supported {
                    if !entry.supported.contains(supported) {
                        entry.supported.push(supported.clone());
                    }
                }
            }
            None => failures.push(Failure {
                position,
                word: word.clone(),
                supported: walk.supported.clone(),
            }),
        }
    }
    failures.sort_by_key(|failure| failure.position);
    for failure in &mut failures {
        failure.supported.sort();
    }
    failures
}

fn answer_for(panel: &Panel, words: &[String], failure: &Failure) -> Option<Answer> {
    let path = command_path(words, failure.position);
    // A word that is a command of its own is not a subcommand of this one:
    // `effigy container exec …` is `effigy exec …`.
    if failure.position > 0 {
        if let Some(topic) = topic_for_word(&failure.word) {
            if let Some(form) = Panel::of(topic)
                .usage
                .iter()
                .find(|line| !line.contains("--json "))
            {
                return Some(Answer {
                    command: path.clone(),
                    supported: failure.supported.clone(),
                    usage: Vec::new(),
                    did_you_mean: Some(format!(
                        "`{path} {}` does not exist: `{}` is its own command — `{form}`",
                        failure.word, failure.word
                    )),
                });
            }
        }
    }
    // A near miss among the words this command accepts at that position.
    let mut best: Option<(usize, &String)> = None;
    for supported in &failure.supported {
        if supported.starts_with('-') {
            continue;
        }
        let distance = distance(&failure.word, supported);
        if best.is_none_or(|(known, _)| distance < known) {
            best = Some((distance, supported));
        }
    }
    let (distance, supported) = best?;
    if distance == 0 || distance > close_match_limit(&failure.word) {
        return None;
    }
    // The supported *word* is the suggestion. Its own usage line carries
    // whatever the word still needs (`container stats` needs `--global`), so
    // the answer never spells a form that cannot run by itself.
    let usage = panel
        .usage
        .iter()
        .find(|line| {
            line_covers(line, &words[..failure.position])
                && words_for_line(line).iter().any(|word| word == supported)
        })
        .cloned();
    Some(Answer {
        command: path.clone(),
        supported: failure.supported.clone(),
        usage: usage.into_iter().collect(),
        did_you_mean: Some(format!(
            "`{path} {}` is not a form: did you mean `{supported}`?",
            failure.word
        )),
    })
}

fn close_match_limit(word: &str) -> usize {
    match word.chars().count() {
        0..=3 => 1,
        4..=6 => 3,
        _ => 4,
    }
}

/// The typed path up to `position`, as `effigy <words>`.
fn command_path(words: &[String], position: usize) -> String {
    let mut path = String::from("effigy");
    for word in words.iter().take(position) {
        path.push(' ');
        path.push_str(word);
    }
    path
}

fn unknown_flag(panel: &Panel, flags: &[String]) -> Option<String> {
    for flag in flags {
        let name = flag.trim_end_matches('=');
        let known = panel
            .options
            .iter()
            .map(String::as_str)
            .chain(panel.usage.iter().flat_map(|line| line.split_whitespace()))
            .filter_map(flag_name)
            .any(|candidate| candidate == name);
        if known {
            continue;
        }
        let mut best: Option<(usize, String)> = None;
        for candidate in panel.options.iter().filter_map(|option| flag_name(option)) {
            let distance = distance(name, candidate);
            if best.as_ref().is_none_or(|(known, _)| distance < *known) {
                best = Some((distance, candidate.to_owned()));
            }
        }
        if let Some((distance, candidate)) = best {
            if distance <= close_match_limit(name) {
                return Some(format!(
                    "`{name}` is not an option here: did you mean `{candidate}`?"
                ));
            }
        }
        return None;
    }
    None
}

/// The `--flag` part of an option row (`--repo <PATH>` -> `--repo`).
fn flag_name(spelling: &str) -> Option<&str> {
    let spelling = spelling.trim();
    if !spelling.starts_with("--") {
        return None;
    }
    let end = spelling.find([' ', '=', '|']).unwrap_or(spelling.len());
    Some(&spelling[..end])
}

/// Command words and flags, in typed order, without `effigy` itself.
fn split_argv(argv: &[String]) -> (Vec<String>, Vec<String>) {
    let mut words = Vec::new();
    let mut flags = Vec::new();
    for arg in argv {
        if arg.starts_with('-') {
            if arg != "--" {
                flags.push(arg.clone());
            }
        } else if arg != "effigy" {
            words.push(arg.clone());
        }
    }
    (words, flags)
}

fn words_for_line(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_owned).collect()
}

/// Whether `line`'s command shape can cover the typed `words` prefix.
fn line_covers(line: &str, words: &[String]) -> bool {
    cover_len(&tokens_for_line(line), words) >= words.len()
}

/// How many typed words the line's command shape consumes.
fn cover_len(tokens: &[Token], words: &[String]) -> usize {
    let mut consumed = 0;
    for token in tokens {
        if consumed == words.len() {
            break;
        }
        let word = &words[consumed];
        match &token.kind {
            Kind::Flag => {}
            Kind::Placeholder => {
                if word.starts_with('-') {
                    continue;
                }
                consumed += 1;
            }
            Kind::Word(expected) => {
                if expected != word {
                    break;
                }
                consumed += 1;
            }
        }
    }
    consumed
}

fn topic_for_word(word: &str) -> Option<HelpTopic> {
    match word {
        "run" => Some(HelpTopic::Runs),
        _ => crate::command_surface::help_topic_for_help_argument(word),
    }
}

/// One help panel's usage lines and option rows, captured once.
struct Panel {
    usage: Vec<String>,
    options: Vec<String>,
}

impl Panel {
    fn of(topic: HelpTopic) -> Self {
        let mut collector = PanelCollector::default();
        // A panel that cannot render has no usage lines: the hint then only
        // points at `effigy --help`, never at the whole help text.
        let _ = render_help(&mut collector, topic);
        Self {
            usage: collector.usage,
            options: collector.options,
        }
    }

    fn walk(&self, words: &[String]) -> Vec<Walk> {
        self.usage
            .iter()
            .map(|line| Walk::of(&tokens_for_line(line), words))
            .collect()
    }

    /// Up to two usage lines that best describe the command resolved so far.
    fn nearest_usage(&self, words: &[String], position: usize) -> Vec<String> {
        let typed = &words[..position.min(words.len())];
        let mut ranked: Vec<(usize, usize, &String)> = self
            .usage
            .iter()
            .enumerate()
            .map(|(index, line)| (cover_len(&tokens_for_line(line), typed), index, line))
            .collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let best = ranked.first().map(|entry| entry.0).unwrap_or(0);
        ranked
            .into_iter()
            .filter(|entry| entry.0 == best)
            .take(2)
            .map(|entry| entry.2.clone())
            .collect()
    }
}

#[derive(Default)]
struct PanelCollector {
    section: Option<String>,
    usage: Vec<String>,
    options: Vec<String>,
}

impl PanelCollector {
    fn in_usage(&self) -> bool {
        self.section
            .as_deref()
            .is_some_and(|title| title.eq_ignore_ascii_case("usage"))
    }

    fn in_options(&self) -> bool {
        self.section
            .as_deref()
            .is_some_and(|title| title.eq_ignore_ascii_case("options"))
    }
}

impl HelpRenderer for PanelCollector {
    fn text(&mut self, body: &str) -> HelpResult<()> {
        let body = body.trim();
        if self.in_usage() && !body.is_empty() {
            self.usage.push(body.to_owned());
        }
        Ok(())
    }

    fn section(&mut self, title: &str) -> HelpResult<()> {
        self.section = Some(title.to_owned());
        Ok(())
    }

    fn notice(&mut self, _level: NoticeLevel, _body: &str) -> HelpResult<()> {
        Ok(())
    }

    fn bullet_list(&mut self, _title: &str, _items: &[String]) -> HelpResult<()> {
        Ok(())
    }

    fn table(&mut self, spec: &TableSpec) -> HelpResult<()> {
        if self.in_options() {
            for row in &spec.rows {
                if let Some(first) = row.first() {
                    self.options.push(first.clone());
                }
            }
        }
        Ok(())
    }

    fn key_values(&mut self, items: &[KeyValue]) -> HelpResult<()> {
        if self.in_options() {
            for item in items {
                self.options.push(item.key.clone());
            }
        }
        Ok(())
    }
}

/// One usage line and how far the typed words match it.
#[derive(Debug, Clone)]
struct Walk {
    /// The first typed word this line cannot account for.
    failure: Option<usize>,
    /// Words this line accepts at the failure position.
    supported: Vec<String>,
}

impl Walk {
    fn of(tokens: &[Token], words: &[String]) -> Self {
        let mut matched = 0;
        for (index, word) in words.iter().enumerate() {
            // Flags never consume a command word.
            while matches!(
                tokens.get(matched).map(|token| &token.kind),
                Some(Kind::Flag)
            ) {
                matched += 1;
            }
            let Some(token) = tokens.get(matched) else {
                return Self {
                    failure: Some(index),
                    supported: Vec::new(),
                };
            };
            match &token.kind {
                Kind::Flag => unreachable!("flags were skipped above"),
                Kind::Placeholder => {
                    if word.starts_with('-') {
                        return Self {
                            failure: Some(index),
                            supported: Vec::new(),
                        };
                    }
                    matched += 1;
                }
                Kind::Word(expected) => {
                    if expected != word {
                        return Self {
                            failure: Some(index),
                            supported: vec![expected.clone()],
                        };
                    }
                    matched += 1;
                }
            }
        }
        Self {
            failure: None,
            supported: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
}

#[derive(Debug, Clone)]
enum Kind {
    Word(String),
    Flag,
    Placeholder,
}

fn tokens_for_line(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    for raw in line.split_whitespace() {
        if raw == "effigy" {
            continue;
        }
        let token = raw.trim_matches(['[', ']']);
        if token.is_empty() {
            continue;
        }
        let kind = if token.contains('<') {
            Kind::Placeholder
        } else if token.starts_with('-') {
            Kind::Flag
        } else {
            Kind::Word(token.split('|').next().unwrap_or(token).to_owned())
        };
        tokens.push(Token { kind });
    }
    tokens
}

/// Levenshtein distance between `a` and `b`.
fn distance(a: &str, b: &str) -> usize {
    let left: Vec<char> = a.chars().collect();
    let right: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (i, left_char) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let cost = usize::from(left_char != right_char);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(argv: &[&str]) -> UsageHint {
        usage_hint_for(
            &argv
                .iter()
                .map(|word| (*word).to_owned())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn a_container_exec_names_the_exec_command_instead_of_the_help_panel() {
        let hint = hint(&["container", "exec", "app", "--", "true"]);
        let did_you_mean = hint.did_you_mean.clone().expect("a targeted answer");
        assert!(
            did_you_mean.contains("`effigy container exec` does not exist"),
            "{did_you_mean}"
        );
        assert!(did_you_mean.contains("`effigy exec"), "{did_you_mean}");
        assert_eq!(hint.command, "effigy container");
        assert!(hint.help_hint().contains("effigy container --help"));
    }

    #[test]
    fn a_near_miss_subcommand_is_a_did_you_mean() {
        let hint = hint(&["container", "stat"]);
        assert_eq!(hint.command, "effigy container");
        let did_you_mean = hint.did_you_mean.clone().expect("a did you mean");
        assert!(
            did_you_mean.contains("did you mean `stats`"),
            "{did_you_mean}"
        );
        assert!(
            hint.usage
                .iter()
                .any(|line| line.contains("effigy container stats --global")),
            "{hint:?}"
        );
    }

    #[test]
    fn an_unknown_subcommand_lists_the_words_the_command_accepts() {
        let hint = hint(&["container", "wibble"]);
        assert!(hint.did_you_mean.is_none(), "{hint:?}");
        assert!(hint.supported.contains(&"up".to_owned()), "{hint:?}");
        assert!(hint.supported.contains(&"volume".to_owned()), "{hint:?}");
        // `shell` only exists as `container <NAME> shell`, so it is not a
        // word this position accepts.
        assert!(!hint.supported.contains(&"shell".to_owned()), "{hint:?}");
        assert!(!hint.supported.contains(&"exec".to_owned()), "{hint:?}");
        assert!(hint.usage.is_empty(), "{hint:?}");
        assert_eq!(hint.command, "effigy container");
    }

    #[test]
    fn an_unknown_flag_is_a_did_you_mean_against_the_options_panel() {
        let hint = hint(&["tasks", "--al"]);
        let did_you_mean = hint.did_you_mean.clone().expect("a did you mean");
        assert!(
            did_you_mean.contains("did you mean `--all`"),
            "{did_you_mean}"
        );
    }

    #[test]
    fn a_missing_value_keeps_the_nearest_commands_usage() {
        let hint = hint(&["tasks", "--repo"]);
        assert!(!hint.usage.is_empty(), "{hint:?}");
        assert!(hint.usage.iter().all(|line| line.contains("effigy tasks")));
        assert_eq!(hint.command, "effigy tasks");
    }

    #[test]
    fn an_unplaceable_first_word_only_points_at_the_general_help() {
        let hint = hint(&["wibble"]);
        assert_eq!(hint.command, "");
        assert_eq!(
            hint.help_hint(),
            "Run `effigy --help` to see supported command forms"
        );
    }

    #[test]
    fn every_topic_panel_renders_its_own_usage_lines() {
        for topic in [
            HelpTopic::Container,
            HelpTopic::Tasks,
            HelpTopic::Secrets,
            HelpTopic::Release,
            HelpTopic::State,
            HelpTopic::Runs,
            HelpTopic::Init,
        ] {
            let lines = usage_lines(topic);
            assert!(!lines.is_empty(), "{topic:?} has usage lines");
            assert!(
                lines.iter().all(|line| line.starts_with("effigy ")),
                "{topic:?}: {lines:?}"
            );
        }
    }

    #[test]
    fn a_topic_usage_line_only_names_words_its_own_parser_accepts() {
        // `container exec` is not a container action; nothing in the panel
        // advertises it, so the suggestion above must come from `exec` itself.
        let container = usage_lines(HelpTopic::Container).join("\n");
        assert!(!container.contains("container exec"));
    }
}
