//! The user-global config view: path, key lookup, rendering and the update
//! that writes or removes `effigy`'s machine-level config file.

use std::path::Path;

use crate::config::output::payload::{render_config_payload, ConfigPayload};
use crate::config::request::{ConfigRequest, UserConfigKey};
use crate::BuiltinError;
use effigy_manifest::{
    load_user_config, save_user_config, user_config_path, UserConfig,
    UserContainerBackendPreference,
};

pub(super) fn render_user_path_payload(
    request: ConfigRequest,
) -> Result<Option<String>, BuiltinError> {
    let path = user_config_path().ok_or_else(|| {
        BuiltinError::task_invocation("HOME is not set; cannot resolve user-global config path")
    })?;
    let text = format!("{}\n", path.display());
    render_config_payload(
        request.output_json,
        ConfigPayload::user_path(text, path.display().to_string()),
    )
}

pub(super) fn render_user_get_payload(
    request: ConfigRequest,
    key: UserConfigKey,
) -> Result<Option<String>, BuiltinError> {
    let path = user_config_path().ok_or_else(|| {
        BuiltinError::task_invocation("HOME is not set; cannot resolve user-global config path")
    })?;
    let config = load_user_config()?;
    let value = render_user_config_key_value(&config, key).ok_or_else(|| {
        BuiltinError::task_invocation(format!(
            "user-global config key `{}` is not set",
            key.as_str()
        ))
    })?;
    let text = format!("{value}\n");
    render_config_payload(
        request.output_json,
        ConfigPayload::user_get(
            text,
            path.display().to_string(),
            &config,
            key.as_str().to_owned(),
            value,
        ),
    )
}

pub(super) fn render_user_inspect_payload(
    request: ConfigRequest,
) -> Result<Option<String>, BuiltinError> {
    let path = user_config_path().ok_or_else(|| {
        BuiltinError::task_invocation("HOME is not set; cannot resolve user-global config path")
    })?;
    let config = load_user_config()?;
    let text = render_user_config_text(&path, &config, false)?;
    render_config_payload(
        request.output_json,
        ConfigPayload::user_inspect(text, path.display().to_string(), &config),
    )
}

pub(super) fn render_user_update_payload(
    request: ConfigRequest,
) -> Result<Option<String>, BuiltinError> {
    let path = user_config_path().ok_or_else(|| {
        BuiltinError::task_invocation("HOME is not set; cannot resolve user-global config path")
    })?;
    let mut config = load_user_config()?;

    if let Some(backend) = request.set_container_backend {
        config.containers.backend = Some(backend);
    }
    if request.unset_container_backend {
        config.containers.backend = None;
    }
    if let Some(profile) = request.set_container_profile.as_ref() {
        config.containers.profile = Some(profile.clone());
    }
    if request.unset_container_profile {
        config.containers.profile = None;
    }
    if let Some(disk_gib) = request.set_container_profile_disk_gib {
        config.containers.profile_disk_gib = Some(disk_gib);
    }
    if request.unset_container_profile_disk_gib {
        config.containers.profile_disk_gib = None;
    }

    let removed_file = if config.is_empty() {
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|error| BuiltinError::task_invocation_failed_write(&path, error))?;
        }
        true
    } else {
        let written_path = save_user_config(&config)?;
        debug_assert_eq!(written_path, path);
        false
    };

    let text = render_user_config_text(&path, &config, removed_file)?;
    render_config_payload(
        request.output_json,
        ConfigPayload::user_update(text, path.display().to_string(), &config, removed_file),
    )
}

fn render_user_config_text(
    path: &Path,
    config: &UserConfig,
    removed_file: bool,
) -> Result<String, BuiltinError> {
    let mut out = String::new();
    out.push_str("User Config\n");
    out.push_str("===========\n\n");
    out.push_str(&format!("Path: {}\n", path.display()));
    if removed_file {
        out.push_str("Status: removed empty config file\n\n");
    } else if path.exists() {
        out.push_str("Status: present\n\n");
    } else {
        out.push_str("Status: not yet created\n\n");
    }

    out.push_str("Container Preferences\n");
    out.push_str("---------------------\n");
    out.push_str(&format!(
        "backend: {}\n",
        config
            .preferred_container_backend()
            .map(render_backend_preference)
            .unwrap_or("(unset)")
    ));
    out.push_str(&format!(
        "profile: {}\n",
        config.preferred_container_profile().unwrap_or("(unset)")
    ));
    out.push_str(&format!(
        "profile_disk_gib: {}\n",
        config
            .preferred_container_profile_disk_gib()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "(unset)".to_owned())
    ));

    if !config.bundle.is_empty() {
        out.push('\n');
        out.push_str("Rendered TOML\n");
        out.push_str("-------------\n");
        out.push_str(&render_user_config_toml(config)?);
    }

    Ok(out)
}

fn render_backend_preference(backend: UserContainerBackendPreference) -> &'static str {
    match backend {
        UserContainerBackendPreference::Containerd => "containerd",
        UserContainerBackendPreference::Docker => "docker",
    }
}

fn render_user_config_key_value(config: &UserConfig, key: UserConfigKey) -> Option<String> {
    match key {
        UserConfigKey::Backend => config
            .preferred_container_backend()
            .map(render_backend_preference)
            .map(str::to_owned),
        UserConfigKey::Profile => config.preferred_container_profile().map(str::to_owned),
        UserConfigKey::ProfileDiskGib => config
            .preferred_container_profile_disk_gib()
            .map(|value| value.to_string()),
    }
}

fn render_user_config_toml(config: &UserConfig) -> Result<String, BuiltinError> {
    toml::to_string_pretty(config).map_err(|error| {
        BuiltinError::task_invocation(format!("failed to render user-global config: {error}"))
    })
}

pub(super) fn serialize_user_config(config: &UserConfig) -> serde_json::Value {
    serde_json::to_value(config).unwrap_or(serde_json::Value::Null)
}
