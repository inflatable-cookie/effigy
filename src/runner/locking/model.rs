#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::runner) enum LockScope {
    Workspace,
    Shared(String),
    Task(String),
    Profile { task: String, profile: String },
}

impl LockScope {
    pub(in crate::runner) fn parse(value: &str) -> Option<Self> {
        let raw = value.trim();
        if raw == "workspace" {
            return Some(Self::Workspace);
        }
        if let Some(name) = raw.strip_prefix("shared:") {
            let name = name.trim();
            if !name.is_empty() {
                return Some(Self::Shared(name.to_owned()));
            }
            return None;
        }
        if let Some(task) = raw.strip_prefix("task:") {
            let task = task.trim();
            if !task.is_empty() {
                return Some(Self::Task(task.to_owned()));
            }
            return None;
        }
        if let Some(rest) = raw.strip_prefix("profile:") {
            let rest = rest.trim();
            let (task, profile) = rest.split_once('/')?;
            let task = task.trim();
            let profile = profile.trim();
            if task.is_empty() || profile.is_empty() {
                return None;
            }
            return Some(Self::Profile {
                task: task.to_owned(),
                profile: profile.to_owned(),
            });
        }
        None
    }

    pub(in crate::runner) fn label(&self) -> String {
        match self {
            Self::Workspace => "workspace".to_owned(),
            Self::Shared(name) => format!("shared:{name}"),
            Self::Task(task) => format!("task:{task}"),
            Self::Profile { task, profile } => format!("profile:{task}/{profile}"),
        }
    }

    pub(in crate::runner) fn file_name(&self) -> String {
        format!("{}.lock", sanitize_for_file_name(&self.label()))
    }
}

fn sanitize_for_file_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '-',
        })
        .collect::<String>()
}
