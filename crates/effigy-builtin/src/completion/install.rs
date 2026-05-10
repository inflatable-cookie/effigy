use std::fs;
use std::path::{Path, PathBuf};

use crate::BuiltinError;

use super::scripts::CompletionShell;

const MANAGED_BLOCK_START: &str = "# >>> effigy completion >>>";
const MANAGED_BLOCK_END: &str = "# <<< effigy completion <<<";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CompletionInstallPlan {
    pub(super) shell: CompletionShell,
    pub(super) script: String,
    pub(super) install_path: PathBuf,
    pub(super) startup_path: Option<PathBuf>,
    pub(super) startup_block: Option<String>,
    pub(super) requires_startup_edit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CompletionInstallResult {
    pub(super) shell: CompletionShell,
    pub(super) script: String,
    pub(super) install_path: PathBuf,
    pub(super) startup_path: Option<PathBuf>,
    pub(super) startup_changed: bool,
    pub(super) startup_managed: bool,
    pub(super) install_changed: bool,
    pub(super) follow_up_required: bool,
    pub(super) follow_up_message: Option<String>,
}

pub(super) fn plan_completion_install(
    shell: CompletionShell,
    script: String,
) -> Result<CompletionInstallPlan, BuiltinError> {
    let home = home_dir().ok_or_else(|| {
        BuiltinError::task_invocation("HOME is not set; cannot resolve completion install paths")
    })?;
    match shell {
        CompletionShell::Bash => {
            let install_path = home
                .join(".local")
                .join("share")
                .join("bash-completion")
                .join("completions")
                .join("effigy");
            let startup_path = home.join(".bashrc");
            let startup_block = format!(
                "{MANAGED_BLOCK_START}\nif [ -f \"$HOME/.local/share/bash-completion/completions/effigy\" ]; then\n  . \"$HOME/.local/share/bash-completion/completions/effigy\"\nfi\n{MANAGED_BLOCK_END}"
            );
            Ok(CompletionInstallPlan {
                shell,
                script,
                install_path,
                startup_path: Some(startup_path),
                startup_block: Some(startup_block),
                requires_startup_edit: true,
            })
        }
        CompletionShell::Zsh => {
            let zdotdir = zdotdir_or_home();
            let install_path = zdotdir.join(".zfunc").join("_effigy");
            let startup_path = zdotdir.join(".zshrc");
            let startup_block = format!(
                "{MANAGED_BLOCK_START}\nfpath=(\"${{ZDOTDIR:-$HOME}}/.zfunc\" $fpath)\nautoload -Uz compinit\ncompinit\n{MANAGED_BLOCK_END}"
            );
            Ok(CompletionInstallPlan {
                shell,
                script,
                install_path,
                startup_path: Some(startup_path),
                startup_block: Some(startup_block),
                requires_startup_edit: true,
            })
        }
        CompletionShell::Fish => Ok(CompletionInstallPlan {
            shell,
            script,
            install_path: home
                .join(".config")
                .join("fish")
                .join("completions")
                .join("effigy.fish"),
            startup_path: None,
            startup_block: None,
            requires_startup_edit: false,
        }),
    }
}

pub(super) fn install_completion(
    plan: CompletionInstallPlan,
) -> Result<CompletionInstallResult, BuiltinError> {
    let install_changed = write_text_file_if_changed(&plan.install_path, &plan.script)?;
    let (startup_changed, startup_managed) = if let (Some(path), Some(block)) =
        (plan.startup_path.as_ref(), plan.startup_block.as_deref())
    {
        (upsert_managed_block(path, block)?, true)
    } else {
        (false, false)
    };
    Ok(CompletionInstallResult {
        shell: plan.shell,
        script: plan.script,
        install_path: plan.install_path,
        startup_path: plan.startup_path,
        startup_changed,
        startup_managed,
        install_changed,
        follow_up_required: true,
        follow_up_message: Some(follow_up_message(plan.shell, startup_managed)),
    })
}

fn follow_up_message(shell: CompletionShell, startup_managed: bool) -> String {
    match shell {
        CompletionShell::Bash if startup_managed => {
            "Open a new bash session or run `source ~/.bashrc`.".to_owned()
        }
        CompletionShell::Zsh if startup_managed => {
            "Open a new zsh session or run `source ${ZDOTDIR:-$HOME}/.zshrc`.".to_owned()
        }
        CompletionShell::Fish => {
            "Open a new fish session or run `source ~/.config/fish/completions/effigy.fish`."
                .to_owned()
        }
        _ => "Open a new shell session to load the updated completion.".to_owned(),
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn zdotdir_or_home() -> PathBuf {
    std::env::var_os("ZDOTDIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(home_dir)
        .unwrap_or_else(|| PathBuf::from("~"))
}

fn write_text_file_if_changed(path: &Path, content: &str) -> Result<bool, BuiltinError> {
    if fs::read_to_string(path).ok().as_deref() == Some(content) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| BuiltinError::task_invocation_failed_write(parent, error))?;
    }
    write_atomic(path, content)?;
    Ok(true)
}

fn upsert_managed_block(path: &Path, block: &str) -> Result<bool, BuiltinError> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    let updated = replace_or_append_managed_block(&existing, block);
    if updated == existing {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| BuiltinError::task_invocation_failed_write(parent, error))?;
    }
    write_atomic(path, &updated)?;
    Ok(true)
}

fn replace_or_append_managed_block(existing: &str, block: &str) -> String {
    if let Some(start) = existing.find(MANAGED_BLOCK_START) {
        if let Some(end_offset) = existing[start..].find(MANAGED_BLOCK_END) {
            let end = start + end_offset + MANAGED_BLOCK_END.len();
            let mut updated = String::new();
            updated.push_str(existing[..start].trim_end());
            if !updated.is_empty() {
                updated.push_str("\n\n");
            }
            updated.push_str(block);
            let suffix = existing[end..].trim_start_matches('\n');
            if !suffix.trim().is_empty() {
                updated.push_str("\n\n");
                updated.push_str(suffix.trim_start());
            }
            if !updated.ends_with('\n') {
                updated.push('\n');
            }
            return updated;
        }
    }
    let mut updated = existing.trim_end().to_owned();
    if !updated.is_empty() {
        updated.push_str("\n\n");
    }
    updated.push_str(block);
    updated.push('\n');
    updated
}

fn write_atomic(path: &Path, content: &str) -> Result<(), BuiltinError> {
    let temp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("effigy")
    ));
    fs::write(&temp_path, content)
        .map_err(|error| BuiltinError::task_invocation_failed_write(&temp_path, error))?;
    fs::rename(&temp_path, path)
        .map_err(|error| BuiltinError::task_invocation_failed_write(path, error))
}

#[cfg(test)]
mod tests {
    use super::{
        plan_completion_install, replace_or_append_managed_block, CompletionShell,
        MANAGED_BLOCK_END, MANAGED_BLOCK_START,
    };

    #[test]
    fn replace_or_append_managed_block_appends_once() {
        let block = format!("{MANAGED_BLOCK_START}\nhello\n{MANAGED_BLOCK_END}");
        let updated = replace_or_append_managed_block("export PATH=\"$HOME/bin:$PATH\"\n", &block);
        assert!(updated.contains("export PATH=\"$HOME/bin:$PATH\""));
        assert!(updated.contains(&block));
    }

    #[test]
    fn replace_or_append_managed_block_replaces_existing_block() {
        let old = format!("{MANAGED_BLOCK_START}\nold\n{MANAGED_BLOCK_END}");
        let new = format!("{MANAGED_BLOCK_START}\nnew\n{MANAGED_BLOCK_END}");
        let updated = replace_or_append_managed_block(&format!("a\n\n{old}\n\nb\n"), &new);
        assert!(updated.contains("\nnew\n"));
        assert!(!updated.contains("\nold\n"));
        assert!(updated.contains("a"));
        assert!(updated.contains("b"));
    }

    #[test]
    fn plan_completion_install_uses_shell_specific_user_local_paths() {
        std::env::set_var("HOME", "/tmp/effigy-home");
        std::env::remove_var("ZDOTDIR");
        let bash =
            plan_completion_install(CompletionShell::Bash, "bash".to_owned()).expect("bash plan");
        assert_eq!(
            bash.install_path,
            std::path::PathBuf::from(
                "/tmp/effigy-home/.local/share/bash-completion/completions/effigy"
            )
        );
        let zsh =
            plan_completion_install(CompletionShell::Zsh, "zsh".to_owned()).expect("zsh plan");
        assert_eq!(
            zsh.install_path,
            std::path::PathBuf::from("/tmp/effigy-home/.zfunc/_effigy")
        );
        let fish =
            plan_completion_install(CompletionShell::Fish, "fish".to_owned()).expect("fish plan");
        assert_eq!(
            fish.install_path,
            std::path::PathBuf::from("/tmp/effigy-home/.config/fish/completions/effigy.fish")
        );
    }
}
