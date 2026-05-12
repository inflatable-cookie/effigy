use std::path::Path;
use std::sync::Arc;

use effigy_core::path_error_text::{failed_to_read_path, failed_to_write_path};
use effigy_env::dotenv::parse_dotenv_entries;
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map};
use ring::digest::{digest, SHA256};

use crate::surface::MODULE_FS;

use super::{
    allocate_temp_dir, dynamic_array_to_strings, resolve_runtime_path, rhai_runtime_error,
    ScriptContext,
};

pub(super) fn register_fs_module(engine: &mut Engine, context: Arc<ScriptContext>) {
    engine.register_static_module(MODULE_FS, std::rc::Rc::new(build_fs_module(context)));
}

fn build_fs_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    let file_context = context.clone();
    module.set_native_fn(
        "read_file",
        move |path: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "read_lines",
        move |path: ImmutableString| -> Result<Array, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            Ok(contents
                .lines()
                .map(|line| line.to_owned().into())
                .collect())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "write_file",
        move |path: ImmutableString, contents: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            std::fs::write(&path, contents.as_str())
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "append_file",
        move |path: ImmutableString, contents: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;
            use std::io::Write;
            file.write_all(contents.as_bytes())
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "write_lines",
        move |path: ImmutableString, lines: Array| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            let rendered = dynamic_array_to_strings(&lines)
                .map_err(|error| rhai_runtime_error(error.to_string()))?
                .join("\n");
            let output = if rendered.is_empty() {
                String::new()
            } else {
                format!("{rendered}\n")
            };
            std::fs::write(&path, output)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "copy",
        move |source: ImmutableString,
              destination: ImmutableString|
              -> Result<i64, Box<EvalAltResult>> {
            let source = resolve_runtime_path(&file_context.cwd, source.as_str());
            let destination = resolve_runtime_path(&file_context.cwd, destination.as_str());
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            let copied = std::fs::copy(&source, &destination)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&destination, error)))?;
            i64::try_from(copied)
                .map_err(|_| rhai_runtime_error("copied file size exceeded Rhai integer range"))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "copy_if_missing",
        move |source: ImmutableString,
              destination: ImmutableString|
              -> Result<bool, Box<EvalAltResult>> {
            let source = resolve_runtime_path(&file_context.cwd, source.as_str());
            let destination = resolve_runtime_path(&file_context.cwd, destination.as_str());
            if destination.exists() {
                return Ok(false);
            }
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            std::fs::copy(&source, &destination)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&destination, error)))?;
            Ok(true)
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "move_path",
        move |source: ImmutableString,
              destination: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let source = resolve_runtime_path(&file_context.cwd, source.as_str());
            let destination = resolve_runtime_path(&file_context.cwd, destination.as_str());
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            std::fs::rename(&source, &destination)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&destination, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "replace_in_file",
        move |path: ImmutableString,
              from: ImmutableString,
              to: ImmutableString|
              -> Result<bool, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            let replaced = contents.replace(from.as_str(), to.as_str());
            if replaced == contents {
                return Ok(false);
            }
            std::fs::write(&path, replaced)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;
            Ok(true)
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "exists",
        move |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok(resolve_runtime_path(&file_context.cwd, path.as_str()).exists())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "is_dir",
        move |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok(resolve_runtime_path(&file_context.cwd, path.as_str()).is_dir())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "is_file",
        move |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            Ok(resolve_runtime_path(&file_context.cwd, path.as_str()).is_file())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "file_size",
        move |path: ImmutableString| -> Result<i64, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let metadata = std::fs::metadata(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            i64::try_from(metadata.len())
                .map_err(|_| rhai_runtime_error("file size exceeded Rhai integer range"))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "sha256",
        move |path: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let bytes = std::fs::read(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            Ok(hex_sha256(&bytes))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "is_symlink",
        move |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::symlink_metadata(&path)
                .map(|metadata| metadata.file_type().is_symlink())
                .or_else(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        Ok(false)
                    } else {
                        Err(error)
                    }
                })
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "list",
        move |path: ImmutableString| -> Result<Array, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let mut entries = std::fs::read_dir(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?
                .map(|entry| {
                    entry
                        .map(|entry| entry.path().display().to_string())
                        .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))
                })
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort();
            Ok(entries.into_iter().map(Into::into).collect())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "list_recursive",
        move |path: ImmutableString| -> Result<Array, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            list_recursive_paths(&path, None)
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "list_recursive",
        move |path: ImmutableString, options: Map| -> Result<Array, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let extension = options
                .get("extension")
                .filter(|value| !value.is_unit())
                .map(|value| value.to_string());
            list_recursive_paths(&path, extension.as_deref())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "create_dir",
        move |path: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            std::fs::create_dir_all(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "remove",
        move |path: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
                        std::fs::remove_dir_all(&path)
                    } else {
                        std::fs::remove_file(&path)
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
            .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "create_symlink",
        move |target: ImmutableString, link: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let target = resolve_runtime_path(&file_context.cwd, target.as_str());
            let link = resolve_runtime_path(&file_context.cwd, link.as_str());
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&target, &link)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(&link, error)))
            }
            #[cfg(not(unix))]
            {
                let _ = target;
                let _ = link;
                Err(rhai_runtime_error(
                    "Rhai symlink helpers are only supported on unix hosts".to_owned(),
                ))
            }
        },
    );
    module.set_native_fn(
        "make_temp_dir",
        move |prefix: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let path = allocate_temp_dir(prefix.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            std::fs::create_dir_all(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;
            Ok(path.display().to_string())
        },
    );
    module.set_native_fn(
        "make_temp_file",
        move |prefix: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let dir = allocate_temp_dir(prefix.as_str())
                .map_err(|error| rhai_runtime_error(error.to_string()))?;
            if let Some(parent) = dir.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
            }
            let path = dir.with_extension("tmp");
            std::fs::File::create(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;
            Ok(path.display().to_string())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "env_file_get",
        move |path: ImmutableString, key: ImmutableString| -> Result<String, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if !path.exists() {
                return Ok(String::new());
            }
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            Ok(parse_dotenv_entries(&contents)
                .get(key.as_str())
                .cloned()
                .unwrap_or_default())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "env_file_entries",
        move |path: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if !path.exists() {
                return Ok(Map::new());
            }
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            let mut map = Map::new();
            for (key, value) in parse_dotenv_entries(&contents) {
                map.insert(key.into(), value.into());
            }
            Ok(map)
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "env_file_map",
        move |path: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            if !path.exists() {
                return Ok(Map::new());
            }
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            let mut map = Map::new();
            for (key, value) in parse_dotenv_entries(&contents) {
                map.insert(key.into(), value.into());
            }
            Ok(map)
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "env_file_set",
        move |path: ImmutableString,
              key: ImmutableString,
              value: ImmutableString|
              -> Result<bool, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let changed = update_env_file_entry(&path, key.as_str(), value.as_str())?;
            Ok(changed)
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "env_file_remove",
        move |path: ImmutableString, key: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            remove_env_file_entry(&path, key.as_str())
        },
    );
    let file_context = context.clone();
    module.set_native_fn(
        "env_file_get_detail",
        move |path: ImmutableString, key: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let mut map = Map::new();
            if !path.exists() {
                map.insert("file_exists".into(), Dynamic::from_bool(false));
                map.insert("key_exists".into(), Dynamic::from_bool(false));
                map.insert("value".into(), Dynamic::from(""));
                return Ok(map);
            }
            map.insert("file_exists".into(), Dynamic::from_bool(true));
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
            let entries = parse_dotenv_entries(&contents);
            if let Some(value) = entries.get(key.as_str()) {
                map.insert("key_exists".into(), Dynamic::from_bool(true));
                map.insert("value".into(), Dynamic::from(value.clone()));
            } else {
                map.insert("key_exists".into(), Dynamic::from_bool(false));
                map.insert("value".into(), Dynamic::from(""));
            }
            Ok(map)
        },
    );
    module
}

fn list_recursive_paths(root: &Path, extension: Option<&str>) -> Result<Array, Box<EvalAltResult>> {
    if !root.exists() {
        return Ok(Array::new());
    }
    let normalized_extension = extension.map(|extension| extension.trim_start_matches('.'));
    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.map_err(|error| rhai_runtime_error(error.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        if let Some(extension) = normalized_extension {
            if entry.path().extension().and_then(|value| value.to_str()) != Some(extension) {
                continue;
            }
        }
        entries.push(entry.path().display().to_string());
    }
    entries.sort();
    Ok(entries.into_iter().map(Into::into).collect())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = digest(&SHA256, bytes);
    let mut output = String::with_capacity(digest.as_ref().len() * 2);
    for byte in digest.as_ref() {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn update_env_file_entry(path: &Path, key: &str, value: &str) -> Result<bool, Box<EvalAltResult>> {
    let rendered_entry = format!("{key}={}", render_dotenv_value(value));
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error)))?;
        }
        std::fs::write(path, format!("{rendered_entry}\n"))
            .map_err(|error| rhai_runtime_error(failed_to_write_path(path, error)))?;
        return Ok(true);
    }

    let contents = std::fs::read_to_string(path)
        .map_err(|error| rhai_runtime_error(failed_to_read_path(path, error)))?;
    let mut found = false;
    let mut changed = false;
    let mut rendered_lines = Vec::new();

    for raw_line in contents.lines() {
        if let Some(existing_key) = parse_dotenv_key(raw_line) {
            if existing_key == key {
                found = true;
                if raw_line != rendered_entry {
                    rendered_lines.push(rendered_entry.clone());
                    changed = true;
                } else {
                    rendered_lines.push(raw_line.to_owned());
                }
                continue;
            }
        }
        rendered_lines.push(raw_line.to_owned());
    }

    if !found {
        rendered_lines.push(rendered_entry);
        changed = true;
    }

    if !changed {
        return Ok(false);
    }

    let mut output = rendered_lines.join("\n");
    output.push('\n');
    std::fs::write(path, output)
        .map_err(|error| rhai_runtime_error(failed_to_write_path(path, error)))?;
    Ok(true)
}

fn remove_env_file_entry(path: &Path, key: &str) -> Result<bool, Box<EvalAltResult>> {
    if !path.exists() {
        return Ok(false);
    }

    let contents = std::fs::read_to_string(path)
        .map_err(|error| rhai_runtime_error(failed_to_read_path(path, error)))?;
    let mut changed = false;
    let mut rendered_lines = Vec::new();

    for raw_line in contents.lines() {
        if parse_dotenv_key(raw_line).is_some_and(|existing_key| existing_key == key) {
            changed = true;
            continue;
        }
        rendered_lines.push(raw_line.to_owned());
    }

    if !changed {
        return Ok(false);
    }

    let mut output = rendered_lines.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    std::fs::write(path, output)
        .map_err(|error| rhai_runtime_error(failed_to_write_path(path, error)))?;
    Ok(true)
}

fn parse_dotenv_key(line: &str) -> Option<&str> {
    let mut trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    if let Some(exported) = trimmed.strip_prefix("export ") {
        trimmed = exported.trim_start();
    }
    let (key, _) = trimmed.split_once('=')?;
    let key = key.trim();
    (!key.is_empty()).then_some(key)
}

fn render_dotenv_value(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_owned();
    }
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '@'))
    {
        return value.to_owned();
    }
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}
