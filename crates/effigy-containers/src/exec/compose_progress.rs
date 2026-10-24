//! What one Compose activation is doing, read from the progress the runtime
//! itself prints.
//!
//! `container up` and `container down` change one service at a time, and the
//! runtime already says which: `docker compose` and `nerdctl compose` print
//! progress lines such as
//!
//! ```text
//!  Image postgres:16 Pulling
//!  Container demo-web-1  Creating
//!  Container demo-web-1  Started
//! ```
//!
//! Effigy turns each line into a live step so a person sees `creating web`
//! while it happens instead of one opaque wait ([`ComposeStepEvent`]). The
//! parser is deliberately tolerant: it accepts both the gerund and the past
//! form, an optional `Container`/`Image` prefix, the `✔`/`✘` glyphs Compose
//! draws on a terminal, and ANSI colour. Service logs (`web-1 | …`) are never
//! progress, so they are never mistaken for a step.

/// One per-service progress event a Compose invocation printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeStepEvent {
    Pulling { subject: String },
    Pulled { subject: String },
    Building { subject: String },
    Built { subject: String },
    Creating { subject: String },
    Created { subject: String },
    Starting { subject: String },
    Started { subject: String },
    Stopping { subject: String },
    Stopped { subject: String },
    Removing { subject: String },
    Removed { subject: String },
    Recreated { subject: String },
}

impl ComposeStepEvent {
    /// The image, container or service the runtime named.
    pub fn subject(&self) -> &str {
        match self {
            Self::Pulling { subject }
            | Self::Pulled { subject }
            | Self::Building { subject }
            | Self::Built { subject }
            | Self::Creating { subject }
            | Self::Created { subject }
            | Self::Starting { subject }
            | Self::Started { subject }
            | Self::Stopping { subject }
            | Self::Stopped { subject }
            | Self::Removing { subject }
            | Self::Removed { subject }
            | Self::Recreated { subject } => subject,
        }
    }

    /// The service this event is about. Compose names the service for
    /// `Image`/build lines and the container for create/start lines, so a
    /// container name is reduced to its service: `<project>-<service>-<n>`.
    pub fn service(&self, project: &str) -> String {
        match self {
            Self::Pulling { .. } | Self::Pulled { .. } => self.subject().to_owned(),
            _ => service_from_container_name(project, self.subject()),
        }
    }

    /// What is happening now, in words: `creating web`.
    pub fn label(&self, project: &str) -> String {
        let service = self.service(project);
        match self {
            Self::Pulling { .. } => format!("pulling {}", image_label(self.subject())),
            Self::Pulled { .. } => format!("pulling {}", image_label(self.subject())),
            Self::Building { .. } | Self::Built { .. } => format!("building {service}"),
            Self::Creating { .. } | Self::Created { .. } => format!("creating {service}"),
            Self::Starting { .. } | Self::Started { .. } => format!("starting {service}"),
            Self::Stopping { .. } | Self::Stopped { .. } => format!("stopping {service}"),
            Self::Removing { .. } | Self::Removed { .. } => format!("removing {service}"),
            Self::Recreated { .. } => format!("recreating {service}"),
        }
    }

    /// The finished step's summary, once the runtime reports the past tense.
    pub fn done_summary(&self, project: &str) -> Option<String> {
        let service = self.service(project);
        let summary = match self {
            Self::Pulled { .. } => format!("pulled {}", image_label(self.subject())),
            Self::Built { .. } => format!("built {service}"),
            Self::Created { .. } => return None,
            Self::Started { .. } => format!("started {service}"),
            Self::Stopped { .. } => return None,
            Self::Removed { .. } => format!("removed {service}"),
            Self::Recreated { .. } => format!("recreated {service}"),
            Self::Pulling { .. }
            | Self::Building { .. }
            | Self::Creating { .. }
            | Self::Starting { .. }
            | Self::Stopping { .. }
            | Self::Removing { .. } => return None,
        };
        Some(summary)
    }

    /// Whether this event ends the work it names.
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            Self::Pulled { .. }
                | Self::Built { .. }
                | Self::Started { .. }
                | Self::Removed { .. }
                | Self::Recreated { .. }
        )
    }

    /// Whether this event is part of a teardown rather than an activation.
    pub fn is_teardown(&self) -> bool {
        matches!(
            self,
            Self::Stopping { .. }
                | Self::Stopped { .. }
                | Self::Removing { .. }
                | Self::Removed { .. }
        )
    }
}

/// The label for an image reference: the repository name without a registry
/// host, so `docker.io/library/postgres:16` reads as `postgres:16`.
fn image_label(image: &str) -> String {
    let without_host = match image.split_once('/') {
        Some((first, rest)) if first.contains('.') || first.contains(':') => rest,
        _ => image,
    };
    without_host
        .strip_prefix("library/")
        .unwrap_or(without_host)
        .to_owned()
}

/// Reduce a runtime container name to the Compose service that owns it.
///
/// Compose names containers `<project>-<service>-<index>` (and nerdctl keeps
/// the `<project>_<service>_<index>` form), so stripping the project prefix
/// and the index leaves the service. Anything that does not match keeps the
/// name the runtime printed: an unrecognised name is better evidence than a
/// guessed service.
pub fn service_from_container_name(project: &str, container: &str) -> String {
    let container = container.trim();
    // Only this project's own naming is understood. A name from another
    // project is kept as printed: a guess would be worse evidence than the
    // runtime's own word.
    let Some(stripped) = container
        .strip_prefix(project)
        .and_then(|rest| rest.strip_prefix('-').or_else(|| rest.strip_prefix('_')))
    else {
        return container.to_owned();
    };
    match stripped.rsplit_once(['-', '_']) {
        Some((service, index))
            if !service.is_empty()
                && !index.is_empty()
                && index.chars().all(|character| character.is_ascii_digit()) =>
        {
            service.to_owned()
        }
        _ => container.to_owned(),
    }
}

/// Read one line of runtime output as a progress event.
///
/// Returns `None` for anything that is not progress: service logs, blank
/// lines, warnings and the prose a runtime prints around its progress.
pub fn parse_compose_step_event(line: &str) -> Option<ComposeStepEvent> {
    let stripped = strip_ansi(line);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return None;
    }
    // A service log line is `web-1 | …`; its text is not a progress verb even
    // when the application happens to print one.
    if trimmed.contains(" | ") || trimmed.contains("\t|") {
        return None;
    }
    let cleaned = trimmed
        .trim_start_matches(['✔', '✘', '!', '⣿', '⠿'])
        .trim()
        .trim_start_matches('.')
        .trim();
    let collapsed = collapse_spaces(cleaned);
    let (subject, verb) = collapsed.rsplit_once(' ')?;
    let verb = normalize_verb(verb)?;
    let subject = strip_subject_prefix(subject.trim());
    if subject.is_empty() {
        return None;
    }
    let subject = subject.to_owned();
    Some(match verb {
        Verb::Pulling => ComposeStepEvent::Pulling { subject },
        Verb::Pulled => ComposeStepEvent::Pulled { subject },
        Verb::Building => ComposeStepEvent::Building { subject },
        Verb::Built => ComposeStepEvent::Built { subject },
        Verb::Creating => ComposeStepEvent::Creating { subject },
        Verb::Created => ComposeStepEvent::Created { subject },
        Verb::Starting => ComposeStepEvent::Starting { subject },
        Verb::Started => ComposeStepEvent::Started { subject },
        Verb::Stopping => ComposeStepEvent::Stopping { subject },
        Verb::Stopped => ComposeStepEvent::Stopped { subject },
        Verb::Removing => ComposeStepEvent::Removing { subject },
        Verb::Removed => ComposeStepEvent::Removed { subject },
        Verb::Recreating => ComposeStepEvent::Creating { subject },
        Verb::Recreated => ComposeStepEvent::Recreated { subject },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Pulling,
    Pulled,
    Building,
    Built,
    Creating,
    Created,
    Starting,
    Started,
    Stopping,
    Stopped,
    Removing,
    Removed,
    Recreating,
    Recreated,
}

fn normalize_verb(word: &str) -> Option<Verb> {
    let lowered = word
        .trim()
        .trim_end_matches(['.', '…'])
        .to_ascii_lowercase();
    Some(match lowered.as_str() {
        "pulling" | "pull" => Verb::Pulling,
        "pulled" => Verb::Pulled,
        "building" | "build" => Verb::Building,
        "built" => Verb::Built,
        "creating" | "create" => Verb::Creating,
        "created" => Verb::Created,
        "starting" | "start" => Verb::Starting,
        "started" => Verb::Started,
        "stopping" | "stop" => Verb::Stopping,
        "stopped" => Verb::Stopped,
        "removing" | "remove" => Verb::Removing,
        "removed" => Verb::Removed,
        "recreating" | "recreate" => Verb::Recreating,
        "recreated" => Verb::Recreated,
        _ => return None,
    })
}

/// Drop the noun Compose puts in front of the subject: `Container demo-web-1`
/// and `Image postgres:16` both name the same kind of thing a step needs.
fn strip_subject_prefix(subject: &str) -> &str {
    // A network or volume is created once for the project, not per service;
    // it is not one of the steps that names a service.
    for prefix in ["Network ", "Volume "] {
        if subject.starts_with(prefix) {
            return "";
        }
    }
    for prefix in ["Container ", "Image ", "Service "] {
        if let Some(rest) = subject.strip_prefix(prefix) {
            return rest.trim();
        }
    }
    subject
}

fn collapse_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Strip ANSI SGR sequences (`ESC [ … m`) and other CSI sequences.
fn strip_ansi(value: &str) -> String {
    let mut stripped = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            stripped.push(character);
            continue;
        }
        // A CSI sequence is `ESC [` … final byte in `@`..=`~`.
        if characters.peek() == Some(&'[') {
            characters.next();
            for character in characters.by_ref() {
                if ('@'..='~').contains(&character) {
                    break;
                }
            }
        }
    }
    stripped
}

#[cfg(test)]
#[path = "compose_progress/tests.rs"]
mod tests;
