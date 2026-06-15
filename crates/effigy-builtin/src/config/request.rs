use effigy_cli::TaskInvocation;
use effigy_manifest::UserContainerBackendPreference;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::BuiltinError;

#[derive(Debug, Clone)]
pub(super) struct ConfigRequest {
    pub(super) inspect: bool,
    pub(super) inspect_path: Option<String>,
    pub(super) schema: bool,
    pub(super) minimal: bool,
    pub(super) output_json: bool,
    pub(super) target: Option<ConfigSchemaTarget>,
    pub(super) runner: Option<ConfigTestRunner>,
    pub(super) user_inspect: bool,
    pub(super) user_path: bool,
    pub(super) user_get: Option<UserConfigKey>,
    pub(super) set_container_backend: Option<UserContainerBackendPreference>,
    pub(super) set_container_profile: Option<String>,
    pub(super) set_container_profile_disk_gib: Option<u64>,
    pub(super) unset_container_backend: bool,
    pub(super) unset_container_profile: bool,
    pub(super) unset_container_profile_disk_gib: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UserConfigKey {
    Backend,
    Profile,
    ProfileDiskGib,
}

impl UserConfigKey {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Backend => "containers.backend",
            Self::Profile => "containers.profile",
            Self::ProfileDiskGib => "containers.profile_disk_gib",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigSchemaTarget {
    Manifest,
    Bundle,
    Demos,
    PackageManager,
    Test,
    Tasks,
    Defer,
    Scan,
    Shell,
}

impl ConfigSchemaTarget {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Bundle => "bundle",
            Self::Demos => "demos",
            Self::PackageManager => "package_manager",
            Self::Test => "test",
            Self::Tasks => "tasks",
            Self::Defer => "defer",
            Self::Scan => "scan",
            Self::Shell => "shell",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigTestRunner {
    Vitest,
    CargoNextest,
    CargoTest,
}

impl ConfigTestRunner {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Vitest => "vitest",
            Self::CargoNextest => "cargo-nextest",
            Self::CargoTest => "cargo-test",
        }
    }
}

const CONFIG_TARGET_CHOICES: [(&str, ConfigSchemaTarget); 9] = [
    ("manifest", ConfigSchemaTarget::Manifest),
    ("bundle", ConfigSchemaTarget::Bundle),
    ("demos", ConfigSchemaTarget::Demos),
    ("package_manager", ConfigSchemaTarget::PackageManager),
    ("test", ConfigSchemaTarget::Test),
    ("tasks", ConfigSchemaTarget::Tasks),
    ("defer", ConfigSchemaTarget::Defer),
    ("scan", ConfigSchemaTarget::Scan),
    ("shell", ConfigSchemaTarget::Shell),
];

const CONFIG_RUNNER_CHOICES: [(&str, ConfigTestRunner); 4] = [
    ("vitest", ConfigTestRunner::Vitest),
    ("nextest", ConfigTestRunner::CargoNextest),
    ("cargo-nextest", ConfigTestRunner::CargoNextest),
    ("cargo-test", ConfigTestRunner::CargoTest),
];

const CONFIG_USER_BACKEND_CHOICES: [(&str, UserContainerBackendPreference); 3] = [
    ("containerd", UserContainerBackendPreference::Containerd),
    ("colima-nerdctl", UserContainerBackendPreference::Containerd),
    ("docker", UserContainerBackendPreference::Docker),
];

pub(super) fn parse_config_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<ConfigRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut inspect = false;
    let mut inspect_path: Option<String> = None;
    let mut schema = false;
    let mut minimal = false;
    let mut output_json = false;
    let mut target: Option<ConfigSchemaTarget> = None;
    let mut runner: Option<ConfigTestRunner> = None;
    let mut user_inspect = false;
    let mut user_path = false;
    let mut user_get: Option<UserConfigKey> = None;
    let mut set_container_backend: Option<UserContainerBackendPreference> = None;
    let mut set_container_profile: Option<String> = None;
    let mut set_container_profile_disk_gib: Option<u64> = None;
    let mut unset_container_backend = false;
    let mut unset_container_profile = false;
    let mut unset_container_profile_disk_gib = false;
    match args.first().map(String::as_str) {
        Some("inspect") => {
            let _ = parser.next();
            inspect = true;
        }
        Some("schema") => {
            let _ = parser.next();
            schema = true;
        }
        Some("path") => {
            let _ = parser.next();
            user_path = true;
        }
        Some("get") => {
            let _ = parser.next();
            let key = parser.required_subcommand(
                "config get",
                "`containers.backend`, `containers.profile`, or `containers.profile_disk_gib`",
            )?;
            user_get = Some(parse_user_config_key("config get", key)?);
        }
        Some("set") => {
            let _ = parser.next();
            let key = parser.required_subcommand(
                "config set",
                "`containers.backend`, `containers.profile`, or `containers.profile_disk_gib`",
            )?;
            match parse_user_config_key("config set", key)? {
                UserConfigKey::Backend => {
                    set_container_backend = Some(parser.builtin_choice_flag_value(
                        "config set",
                        "containers.backend",
                        "containerd, docker",
                        |value| {
                            BuiltinArgParser::choice_ignore_ascii_case(
                                value,
                                &CONFIG_USER_BACKEND_CHOICES,
                            )
                        },
                    )?);
                }
                UserConfigKey::Profile => {
                    set_container_profile = Some(parser.mapped_flag_value(
                        "`config set containers.profile` requires a value",
                        |value| {
                            let trimmed = value.trim();
                            if trimmed.is_empty() {
                                None
                            } else {
                                Some(trimmed.to_owned())
                            }
                        },
                        |_| "invalid `containers.profile` value".to_owned(),
                    )?);
                }
                UserConfigKey::ProfileDiskGib => {
                    set_container_profile_disk_gib = Some(parser.positive_u64_flag_value(
                        "containers.profile_disk_gib",
                        "`config set containers.profile_disk_gib` requires a value",
                    )?);
                }
            }
        }
        Some("unset") => {
            let _ = parser.next();
            let key = parser.required_subcommand(
                "config unset",
                "`containers.backend`, `containers.profile`, or `containers.profile_disk_gib`",
            )?;
            match parse_user_config_key("config unset", key)? {
                UserConfigKey::Backend => unset_container_backend = true,
                UserConfigKey::Profile => unset_container_profile = true,
                UserConfigKey::ProfileDiskGib => {
                    unset_container_profile_disk_gib = true;
                }
            }
        }
        _ => {}
    }
    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_any_bool_flag(
            arg,
            &mut [
                ("--schema", &mut schema),
                ("--inspect", &mut inspect),
                ("--minimal", &mut minimal),
                ("--json", &mut output_json),
                ("--user-inspect", &mut user_inspect),
                ("--user-path", &mut user_path),
                ("--unset-container-backend", &mut unset_container_backend),
                ("--unset-container-profile", &mut unset_container_profile),
                (
                    "--unset-container-profile-disk",
                    &mut unset_container_profile_disk_gib,
                ),
            ],
        ) {
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--target" {
            target = Some(parser.builtin_choice_flag_value(
                "config",
                "--target",
                "manifest, bundle, demos, package_manager, test, tasks, defer, scan, shell",
                |value| BuiltinArgParser::choice_ignore_ascii_case(value, &CONFIG_TARGET_CHOICES),
            )?);
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--path" {
            inspect_path = Some(parser.mapped_flag_value(
                "`--path` requires a value",
                |value| Some(value.to_owned()),
                |_| "invalid `--path` value".to_owned(),
            )?);
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--runner" {
            runner = Some(parser.builtin_choice_flag_value(
                "config",
                "--runner",
                "vitest, cargo-nextest, cargo-test",
                |value| BuiltinArgParser::choice_ignore_ascii_case(value, &CONFIG_RUNNER_CHOICES),
            )?);
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--set-container-backend" {
            set_container_backend = Some(parser.builtin_choice_flag_value(
                "config",
                "--set-container-backend",
                "containerd, docker",
                |value| {
                    BuiltinArgParser::choice_ignore_ascii_case(value, &CONFIG_USER_BACKEND_CHOICES)
                },
            )?);
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--set-container-profile" {
            set_container_profile = Some(parser.mapped_flag_value(
                "`--set-container-profile` requires a value",
                |value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_owned())
                    }
                },
                |_| "invalid `--set-container-profile` value".to_owned(),
            )?);
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--set-container-profile-disk" {
            set_container_profile_disk_gib = Some(parser.positive_u64_flag_value(
                "--set-container-profile-disk",
                "`--set-container-profile-disk` requires a value",
            )?);
            return Ok(ParseLoopAction::Handled);
        }
        Ok(ParseLoopAction::Unknown)
    })?;
    if inspect && schema {
        return Err(BuiltinError::task_invocation(
            "`--inspect` cannot be combined with `--schema` for built-in `config`",
        ));
    }
    if minimal && !schema {
        return Err(BuiltinError::task_invocation(
            "`--minimal` requires `--schema` for built-in `config`",
        ));
    }
    if target.is_some() && !schema {
        return Err(BuiltinError::task_invocation(
            "`--target` requires `--schema` for built-in `config`",
        ));
    }
    if runner.is_some() && !schema {
        return Err(BuiltinError::task_invocation(
            "`--runner` requires `--schema` for built-in `config`",
        ));
    }
    if runner.is_some() && target != Some(ConfigSchemaTarget::Test) {
        return Err(BuiltinError::task_invocation(
            "`--runner` requires `--target test` for built-in `config`",
        ));
    }
    let user_mode = user_inspect
        || user_path
        || user_get.is_some()
        || set_container_backend.is_some()
        || set_container_profile.is_some()
        || set_container_profile_disk_gib.is_some()
        || unset_container_backend
        || unset_container_profile
        || unset_container_profile_disk_gib;
    if user_mode && (inspect || schema) {
        return Err(BuiltinError::task_invocation(
            "user-global config flags cannot be combined with `--inspect` or `--schema` for built-in `config`",
        ));
    }
    if user_mode && inspect_path.is_some() {
        return Err(BuiltinError::task_invocation(
            "`--path` cannot be combined with user-global config flags for built-in `config`",
        ));
    }
    if inspect_path.is_some() && !inspect {
        return Err(BuiltinError::task_invocation(
            "`--path` requires `--inspect` for built-in `config`",
        ));
    }
    if user_inspect
        && (user_path
            || user_get.is_some()
            || set_container_backend.is_some()
            || set_container_profile.is_some()
            || set_container_profile_disk_gib.is_some()
            || unset_container_backend
            || unset_container_profile
            || unset_container_profile_disk_gib)
    {
        return Err(BuiltinError::task_invocation(
            "`--user-inspect` cannot be combined with other user-global config operations for built-in `config`",
        ));
    }
    if user_path
        && (user_get.is_some()
            || set_container_backend.is_some()
            || set_container_profile.is_some()
            || set_container_profile_disk_gib.is_some()
            || unset_container_backend
            || unset_container_profile
            || unset_container_profile_disk_gib)
    {
        return Err(BuiltinError::task_invocation(
            "`path`/`--user-path` cannot be combined with other user-global config operations for built-in `config`",
        ));
    }
    if user_get.is_some()
        && (set_container_backend.is_some()
            || set_container_profile.is_some()
            || set_container_profile_disk_gib.is_some()
            || unset_container_backend
            || unset_container_profile
            || unset_container_profile_disk_gib)
    {
        return Err(BuiltinError::task_invocation(
            "`get` cannot be combined with user-global config update flags for built-in `config`",
        ));
    }
    if set_container_backend.is_some() && unset_container_backend {
        return Err(BuiltinError::task_invocation(
            "`--set-container-backend` cannot be combined with `--unset-container-backend` for built-in `config`",
        ));
    }
    if set_container_profile.is_some() && unset_container_profile {
        return Err(BuiltinError::task_invocation(
            "`--set-container-profile` cannot be combined with `--unset-container-profile` for built-in `config`",
        ));
    }
    if set_container_profile_disk_gib.is_some() && unset_container_profile_disk_gib {
        return Err(BuiltinError::task_invocation(
            "`--set-container-profile-disk` cannot be combined with `--unset-container-profile-disk` for built-in `config`",
        ));
    }

    Ok(ConfigRequest {
        inspect,
        inspect_path,
        schema,
        minimal,
        output_json,
        target,
        runner,
        user_inspect,
        user_path,
        user_get,
        set_container_backend,
        set_container_profile,
        set_container_profile_disk_gib,
        unset_container_backend,
        unset_container_profile,
        unset_container_profile_disk_gib,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigParseContract {
    pub inspect: bool,
    pub inspect_path: Option<String>,
    pub schema: bool,
    pub minimal: bool,
    pub output_json: bool,
    pub target: Option<&'static str>,
    pub bundle: Option<String>,
    pub runner: Option<&'static str>,
    pub user_inspect: bool,
    pub user_path: bool,
    pub user_get: Option<&'static str>,
    pub set_container_backend: Option<&'static str>,
    pub set_container_profile: Option<String>,
    pub set_container_profile_disk_gib: Option<u64>,
    pub unset_container_backend: bool,
    pub unset_container_profile: bool,
    pub unset_container_profile_disk_gib: bool,
}

pub fn parse_config_contract_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<ConfigParseContract, BuiltinError> {
    let parsed = parse_config_request(task, args)?;
    Ok(ConfigParseContract {
        inspect: parsed.inspect,
        inspect_path: parsed.inspect_path,
        schema: parsed.schema,
        minimal: parsed.minimal,
        output_json: parsed.output_json,
        target: parsed.target.map(ConfigSchemaTarget::as_str),
        bundle: None,
        runner: parsed.runner.map(ConfigTestRunner::as_str),
        user_inspect: parsed.user_inspect,
        user_path: parsed.user_path,
        user_get: parsed.user_get.map(UserConfigKey::as_str),
        set_container_backend: parsed.set_container_backend.map(|backend| match backend {
            UserContainerBackendPreference::Containerd => "containerd",
            UserContainerBackendPreference::Docker => "docker",
        }),
        set_container_profile: parsed.set_container_profile,
        set_container_profile_disk_gib: parsed.set_container_profile_disk_gib,
        unset_container_backend: parsed.unset_container_backend,
        unset_container_profile: parsed.unset_container_profile,
        unset_container_profile_disk_gib: parsed.unset_container_profile_disk_gib,
    })
}

fn parse_user_config_key(context: &str, key: &str) -> Result<UserConfigKey, BuiltinError> {
    match key {
        "containers.backend" => Ok(UserConfigKey::Backend),
        "containers.profile" => Ok(UserConfigKey::Profile),
        "containers.profile_disk_gib" => Ok(UserConfigKey::ProfileDiskGib),
        _ => Err(BuiltinError::task_invocation(format!(
            "unknown {context} key `{key}` (expected `containers.backend`, `containers.profile`, or `containers.profile_disk_gib`)"
        ))),
    }
}
