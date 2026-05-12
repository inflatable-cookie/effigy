use std::ffi::OsString;
use std::path::Path;

use effigy_core::shell::shell_quote;

const CONTAINER_HANDOFF_ENV: &str = "EFFIGY_INTERNAL_CONTAINER_HANDOFF=1";
const CONTAINER_WORKSPACE_EFFIGY_BIN_DIR: &str = "/usr/local/bin";
const CONTAINER_COLOR_ENV: [(&str, &str); 3] = [
    ("EFFIGY_COLOR", "always"),
    ("CLICOLOR_FORCE", "1"),
    ("FORCE_COLOR", "3"),
];
const CONTAINER_TTY_COLOR_ENV: [(&str, &str); 2] =
    [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedWorkspaceExecIdentity {
    pub(super) user: String,
    pub(super) home: Option<String>,
}

pub(super) fn build_container_shell_args(
    service: &str,
    command: Option<&str>,
    working_dir: Option<&Path>,
    shell: &str,
    workspace_identity: Option<&ResolvedWorkspaceExecIdentity>,
) -> Vec<OsString> {
    if let Some(command) = command {
        let mut args = vec![OsString::from("exec"), OsString::from("-T")];
        if let Some(working_dir) = working_dir {
            args.push(OsString::from("-w"));
            args.push(OsString::from(working_dir));
        }
        append_workspace_exec_identity(&mut args, workspace_identity);
        append_color_exec_env(&mut args, false);
        args.push(OsString::from("-e"));
        args.push(OsString::from(CONTAINER_HANDOFF_ENV));
        args.push(OsString::from(service));
        args.push(OsString::from("sh"));
        args.push(OsString::from("-lc"));
        args.push(OsString::from(render_effigy_path_prefixed_command(command)));
        return args;
    }

    let mut args = vec![OsString::from("exec")];
    if let Some(working_dir) = working_dir {
        args.push(OsString::from("-w"));
        args.push(OsString::from(working_dir));
    }
    append_workspace_exec_identity(&mut args, workspace_identity);
    append_color_exec_env(&mut args, true);
    args.push(OsString::from("-e"));
    args.push(OsString::from(CONTAINER_HANDOFF_ENV));
    args.push(OsString::from(service));
    args.push(OsString::from(shell));
    args.push(OsString::from("-lc"));
    args.push(OsString::from(render_effigy_path_prefixed_command(
        &format!("exec {} -i", shell_quote(shell)),
    )));
    args
}

pub(super) fn build_interactive_container_shell_args(
    service: &str,
    initial_command: Option<&str>,
    working_dir: Option<&Path>,
    shell: &str,
    workspace_identity: Option<&ResolvedWorkspaceExecIdentity>,
) -> Vec<OsString> {
    let mut args = vec![OsString::from("exec")];
    if let Some(working_dir) = working_dir {
        args.push(OsString::from("-w"));
        args.push(OsString::from(working_dir));
    }
    append_workspace_exec_identity(&mut args, workspace_identity);
    append_color_exec_env(&mut args, true);
    args.push(OsString::from("-e"));
    args.push(OsString::from(CONTAINER_HANDOFF_ENV));
    args.push(OsString::from(service));
    if let Some(command) = initial_command {
        args.push(OsString::from(shell));
        args.push(OsString::from("-lc"));
        args.push(OsString::from(render_interactive_shell_session_command(
            command, shell,
        )));
        return args;
    }
    args.push(OsString::from(shell));
    args.push(OsString::from("-lc"));
    args.push(OsString::from(render_effigy_path_prefixed_command(
        &format!("exec {} -i", shell_quote(shell)),
    )));
    args
}

fn append_workspace_exec_identity(
    args: &mut Vec<OsString>,
    workspace_identity: Option<&ResolvedWorkspaceExecIdentity>,
) {
    if let Some(user) = workspace_identity.map(|identity| identity.user.as_str()) {
        args.push(OsString::from("-u"));
        args.push(OsString::from(user));
    }
    if let Some(home) = workspace_identity.and_then(|identity| identity.home.as_deref()) {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("HOME={home}")));
    }
}

fn append_color_exec_env(args: &mut Vec<OsString>, tty: bool) {
    for (key, value) in CONTAINER_COLOR_ENV {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("{key}={value}")));
    }
    if tty {
        for (key, value) in CONTAINER_TTY_COLOR_ENV {
            args.push(OsString::from("-e"));
            args.push(OsString::from(format!("{key}={value}")));
        }
    }
}

fn render_interactive_shell_session_command(initial_command: &str, shell: &str) -> String {
    render_effigy_path_prefixed_command(&format!(
        "{initial_command}; exec {} -i",
        shell_quote(shell)
    ))
}

fn render_effigy_path_prefixed_command(command: &str) -> String {
    format!(
        "export PATH={}:$PATH; {command}",
        shell_quote(CONTAINER_WORKSPACE_EFFIGY_BIN_DIR)
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_container_shell_args, build_interactive_container_shell_args,
        render_effigy_path_prefixed_command, render_interactive_shell_session_command,
        ResolvedWorkspaceExecIdentity,
    };
    use std::path::Path;

    #[test]
    fn interactive_shell_command_reenters_shell() {
        let rendered = render_interactive_shell_session_command("effigy dev", "/bin/custom shell");
        assert_eq!(
            rendered,
            "export PATH='/usr/local/bin':$PATH; effigy dev; exec '/bin/custom shell' -i"
        );
    }

    #[test]
    fn effigy_path_prefixed_command_prepends_workspace_binary_dir() {
        let rendered = render_effigy_path_prefixed_command("effigy tasks");
        assert_eq!(rendered, "export PATH='/usr/local/bin':$PATH; effigy tasks");
    }

    #[test]
    fn command_mode_shell_exec_disables_nested_tty() {
        let workspace_identity = ResolvedWorkspaceExecIdentity {
            user: "dev".to_owned(),
            home: Some("/home/dev".to_owned()),
        };
        let args = build_container_shell_args(
            "app",
            Some("echo hi"),
            Some(Path::new("/tmp/work")),
            "/bin/sh",
            Some(&workspace_identity),
        );
        let rendered = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(rendered.windows(2).any(|window| window == ["exec", "-T"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-w", "/tmp/work"]));
        assert!(rendered.windows(2).any(|window| window == ["-u", "dev"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "HOME=/home/dev"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "EFFIGY_COLOR=always"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "FORCE_COLOR=3"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "EFFIGY_INTERNAL_CONTAINER_HANDOFF=1"]));
        assert!(rendered.ends_with(&[
            "app".to_owned(),
            "sh".to_owned(),
            "-lc".to_owned(),
            "export PATH='/usr/local/bin':$PATH; echo hi".to_owned(),
        ]));
    }

    #[test]
    fn interactive_shell_exec_keeps_tty_and_sets_working_dir() {
        let args = build_container_shell_args(
            "app",
            None,
            Some(Path::new("/workspace-root/repo")),
            "/bin/bash",
            None,
        );
        let rendered = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(rendered.windows(2).all(|window| window != ["exec", "-T"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-w", "/workspace-root/repo"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "EFFIGY_COLOR=always"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "TERM=xterm-256color"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "COLORTERM=truecolor"]));
        assert!(rendered
            .windows(2)
            .any(|window| window == ["-e", "EFFIGY_INTERNAL_CONTAINER_HANDOFF=1"]));
        assert!(rendered
            .windows(3)
            .any(|window| window == ["app", "/bin/bash", "-lc"]));
        let command = rendered.last().expect("interactive shell command");
        assert!(command.contains("export PATH='/usr/local/bin':$PATH;"));
        assert!(command.contains("exec"));
        assert!(command.contains("/bin/bash"));
        assert!(command.contains("-i"));
    }

    #[test]
    fn interactive_shell_args_include_command_reentry() {
        let args = build_interactive_container_shell_args(
            "app",
            Some("effigy dev"),
            Some(Path::new("/workspace")),
            "/bin/sh",
            None,
        );
        let rendered = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(rendered.contains(&"exec".to_owned()));
        assert!(rendered.contains(&"-w".to_owned()));
        assert!(rendered.contains(&"/workspace".to_owned()));
        assert!(rendered.contains(&"app".to_owned()));
        assert!(rendered.contains(&"/bin/sh".to_owned()));
        assert!(rendered.contains(&"-lc".to_owned()));
        let command = rendered.last().expect("interactive command");
        assert!(command.contains("export PATH='/usr/local/bin':$PATH;"));
        assert!(command.contains("effigy dev;"));
        assert!(command.contains("exec"));
        assert!(command.contains("/bin/sh"));
        assert!(command.contains("-i"));
    }

    #[test]
    fn non_primary_shell_exec_omits_working_dir() {
        let args = build_container_shell_args("app", Some("pwd"), None, "/bin/sh", None);
        let rendered = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(rendered.windows(2).all(|window| window[0] != "-w"));
    }
}
