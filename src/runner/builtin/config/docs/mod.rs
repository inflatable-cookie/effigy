#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigDocProfile {
    Reference,
    Schema,
}

mod sections;
mod tasks;
mod test_runners;

pub(super) fn defer_lines() -> &'static [&'static str] {
    sections::defer_lines()
}

pub(super) fn shell_lines() -> &'static [&'static str] {
    sections::shell_lines()
}

pub(super) fn scan_lines() -> &'static [&'static str] {
    sections::scan_lines()
}

pub(super) fn tasks_minimal_lines() -> &'static [&'static str] {
    sections::tasks_minimal_lines()
}

pub(super) fn package_manager_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    tasks::package_manager_lines(profile)
}

pub(super) fn tasks_canonical_lines(profile: ConfigDocProfile) -> Vec<&'static str> {
    tasks::tasks_canonical_lines(profile)
}

pub(super) fn test_section_lines(
    include_core: bool,
    profile: ConfigDocProfile,
    runner: Option<&str>,
) -> Vec<&'static str> {
    test_runners::test_section_lines(include_core, profile, runner)
}

#[cfg(test)]
mod tests;
