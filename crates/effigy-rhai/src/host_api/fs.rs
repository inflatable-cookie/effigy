use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use effigy_core::path_error_text::{failed_to_read_path, failed_to_write_path};
use effigy_env::dotenv::parse_dotenv_entries;
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map};
use ring::digest::{digest, SHA256};

use crate::surface::MODULE_FS;

use super::{
    allocate_temp_dir, dynamic_array_to_strings, resolve_runtime_path, rhai_runtime_error,
    ScriptContext,
};

/// Create the parent directory a write needs.
fn ensure_parent(path: &Path) -> Result<(), Box<EvalAltResult>> {
    match path.parent() {
        Some(parent) => std::fs::create_dir_all(parent)
            .map_err(|error| rhai_runtime_error(failed_to_write_path(parent, error))),
        None => Ok(()),
    }
}

pub(super) fn register_fs_module(engine: &mut Engine, context: Arc<ScriptContext>) {
    engine.register_static_module(MODULE_FS, std::rc::Rc::new(build_fs_module(context)));
}

/// Register one `fs::` native function with its own context handle.
macro_rules! fs_fn {
    ($module:ident, $context:expr => $local:ident, $name:expr, |$($arg:ident: $ty:ty),*| -> $ret:ty { $($body:tt)* }) => {{
        let $local = $context.clone();
        $module.set_native_fn($name, move |$($arg: $ty),*| -> $ret { $($body)* });
    }};
}

fn build_fs_module(context: Arc<ScriptContext>) -> rhai::Module {
    let mut module = rhai::Module::new();
    fs_fn!(module, context => file_context, "read_file", |path: ImmutableString| -> Result<String, Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        std::fs::read_to_string(&path)
            .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))
    });
    fs_fn!(module, context => file_context, "read_lines", |path: ImmutableString| -> Result<Array, Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        let contents = std::fs::read_to_string(&path)
            .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
        Ok(contents
            .lines()
            .map(|line| line.to_owned().into())
            .collect())
    });
    fs_fn!(module, context => file_context, "write_file", |path: ImmutableString, contents: ImmutableString| -> Result<(), Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        ensure_parent(&path)?;
        std::fs::write(&path, contents.as_str())
            .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
    });
    fs_fn!(module, context => file_context, "append_file", |path: ImmutableString, contents: ImmutableString| -> Result<(), Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        ensure_parent(&path)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;
        use std::io::Write;
        file.write_all(contents.as_bytes())
            .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
    });
    fs_fn!(module, context => file_context, "write_lines", |path: ImmutableString, lines: Array| -> Result<(), Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        ensure_parent(&path)?;
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
    });
    fs_fn!(module, context => file_context, "write_file_if_absent", |path: ImmutableString, contents: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        publish_absent_payload(&path, contents.as_bytes())
    });
    fs_fn!(module, context => file_context, "copy", |source: ImmutableString, destination: ImmutableString| -> Result<i64, Box<EvalAltResult>> {
        let source = resolve_runtime_path(&file_context.cwd, source.as_str());
        let destination = resolve_runtime_path(&file_context.cwd, destination.as_str());
        ensure_parent(&destination)?;
        let copied = std::fs::copy(&source, &destination)
            .map_err(|error| rhai_runtime_error(failed_to_write_path(&destination, error)))?;
        i64::try_from(copied)
            .map_err(|_| rhai_runtime_error("copied file size exceeded Rhai integer range"))
    });
    fs_fn!(module, context => file_context, "copy_if_missing", |source: ImmutableString, destination: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
        let source = resolve_runtime_path(&file_context.cwd, source.as_str());
        let destination = resolve_runtime_path(&file_context.cwd, destination.as_str());
        copy_absent_file(&source, &destination)
    });
    // `std::fs::rename` replaces whatever currently names the destination and
    // never checks that it still matches an inode the caller read, so
    // `move_path` is an atomic replacement, not an identity-checked conditional
    // move. Use `write_file_if_absent` for create-if-absent publication.
    fs_fn!(module, context => file_context, "move_path", |source: ImmutableString, destination: ImmutableString| -> Result<(), Box<EvalAltResult>> {
        let source = resolve_runtime_path(&file_context.cwd, source.as_str());
        let destination = resolve_runtime_path(&file_context.cwd, destination.as_str());
        ensure_parent(&destination)?;
        std::fs::rename(&source, &destination)
            .map_err(|error| rhai_runtime_error(failed_to_write_path(&destination, error)))
    });
    fs_fn!(module, context => file_context, "replace_in_file", |path: ImmutableString, from: ImmutableString, to: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
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
    });
    fs_fn!(module, context => file_context, "exists", |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
        Ok(resolve_runtime_path(&file_context.cwd, path.as_str()).exists())
    });
    fs_fn!(module, context => file_context, "is_dir", |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
        Ok(resolve_runtime_path(&file_context.cwd, path.as_str()).is_dir())
    });
    fs_fn!(module, context => file_context, "is_file", |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
        Ok(resolve_runtime_path(&file_context.cwd, path.as_str()).is_file())
    });
    fs_fn!(module, context => file_context, "file_size", |path: ImmutableString| -> Result<i64, Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        let metadata = std::fs::metadata(&path)
            .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
        i64::try_from(metadata.len())
            .map_err(|_| rhai_runtime_error("file size exceeded Rhai integer range"))
    });
    fs_fn!(module, context => file_context, "sha256", |path: ImmutableString| -> Result<String, Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        let bytes = std::fs::read(&path)
            .map_err(|error| rhai_runtime_error(failed_to_read_path(&path, error)))?;
        Ok(hex_sha256(&bytes))
    });
    fs_fn!(module, context => file_context, "is_symlink", |path: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
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
    });
    fs_fn!(module, context => file_context, "list", |path: ImmutableString| -> Result<Array, Box<EvalAltResult>> {
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
    });
    fs_fn!(module, context => file_context, "list_recursive", |path: ImmutableString| -> Result<Array, Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        list_recursive_paths(&path, None)
    });
    fs_fn!(module, context => file_context, "list_recursive", |path: ImmutableString, options: Map| -> Result<Array, Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        let extension = options
            .get("extension")
            .filter(|value| !value.is_unit())
            .map(|value| value.to_string());
        list_recursive_paths(&path, extension.as_deref())
    });
    fs_fn!(module, context => file_context, "create_dir", |path: ImmutableString| -> Result<(), Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        std::fs::create_dir_all(&path)
            .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))
    });
    fs_fn!(module, context => file_context, "remove", |path: ImmutableString| -> Result<(), Box<EvalAltResult>> {
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
    });
    fs_fn!(module, context => file_context, "create_symlink", |target: ImmutableString, link: ImmutableString| -> Result<(), Box<EvalAltResult>> {
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
    });
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
            ensure_parent(&dir)?;
            let path = dir.with_extension("tmp");
            std::fs::File::create(&path)
                .map_err(|error| rhai_runtime_error(failed_to_write_path(&path, error)))?;
            Ok(path.display().to_string())
        },
    );
    fs_fn!(module, context => file_context, "env_file_get", |path: ImmutableString, key: ImmutableString| -> Result<String, Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        let entries = read_env_entries(&path)?.unwrap_or_default();
        Ok(entries.get(key.as_str()).cloned().unwrap_or_default())
    });
    // `env_file_map` is the older spelling of `env_file_entries`; both names
    // answer from the same projection so they cannot drift.
    for name in ["env_file_entries", "env_file_map"] {
        fs_fn!(module, context => file_context, name, |path: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
            let path = resolve_runtime_path(&file_context.cwd, path.as_str());
            let entries = read_env_entries(&path)?.unwrap_or_default();
            let mut map = Map::new();
            for (key, value) in entries {
                map.insert(key.into(), value.into());
            }
            Ok(map)
        });
    }
    fs_fn!(module, context => file_context, "env_file_set", |path: ImmutableString, key: ImmutableString, value: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        let changed = update_env_file_entry(&path, key.as_str(), value.as_str())?;
        Ok(changed)
    });
    fs_fn!(module, context => file_context, "env_file_remove", |path: ImmutableString, key: ImmutableString| -> Result<bool, Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        remove_env_file_entry(&path, key.as_str())
    });
    fs_fn!(module, context => file_context, "env_file_get_detail", |path: ImmutableString, key: ImmutableString| -> Result<Map, Box<EvalAltResult>> {
        let path = resolve_runtime_path(&file_context.cwd, path.as_str());
        let mut map = Map::new();
        let Some(entries) = read_env_entries(&path)? else {
            map.insert("file_exists".into(), Dynamic::from_bool(false));
            map.insert("key_exists".into(), Dynamic::from_bool(false));
            map.insert("value".into(), Dynamic::from(""));
            return Ok(map);
        };
        map.insert("file_exists".into(), Dynamic::from_bool(true));
        if let Some(value) = entries.get(key.as_str()) {
            map.insert("key_exists".into(), Dynamic::from_bool(true));
            map.insert("value".into(), Dynamic::from(value.clone()));
        } else {
            map.insert("key_exists".into(), Dynamic::from_bool(false));
            map.insert("value".into(), Dynamic::from(""));
        }
        Ok(map)
    });
    module
}

/// Counter that keeps concurrent staged payload names inside one process
/// distinct even when two publications start in the same nanosecond.
static STAGED_PAYLOAD_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Publish a complete payload at `destination` only when no entry exists there.
///
/// The payload is first staged as a fresh file in the destination directory so
/// it shares the destination's filesystem. `hard_link` then creates the
/// destination name atomically and refuses to replace an entry that already
/// exists, so concurrent publishers produce exactly one winner and a reader can
/// never observe a partially written destination. Returns `Ok(true)` when this
/// call published the payload and `Ok(false)` when the destination was already
/// occupied (including by a symlink or directory).
///
/// The operation makes a complete payload visible atomically; it does not fsync
/// the payload or its directory, so durability across power loss is not
/// promised. Filesystems without hard-link support fail with the underlying OS
/// error rather than falling back to a non-atomic write.
fn publish_absent_payload(destination: &Path, contents: &[u8]) -> Result<bool, Box<EvalAltResult>> {
    ensure_parent(destination)?;
    let (staged, mut file) = create_staged_payload(destination)
        .map_err(|error| rhai_runtime_error(failed_to_write_path(destination, error)))?;
    use std::io::Write;
    let write_result = file.write_all(contents).and_then(|()| file.flush());
    drop(file);
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&staged);
        return Err(rhai_runtime_error(failed_to_write_path(&staged, error)));
    }
    publish_staged_no_clobber(&staged, destination)
}

/// Stream `source` into a staged file on the destination filesystem, then
/// publish that complete independent copy with the same atomic no-clobber
/// link as [`publish_absent_payload`].
///
/// The source inode is never linked to the destination: a later write to
/// `source` cannot change a published copy. The copy streams rather than
/// loading the payload into memory. Returns `Ok(true)` only when this call's
/// complete copy is the published destination, and `Ok(false)` when the
/// destination was already occupied (including by a symlink or directory).
/// Occupancy is decided from the destination name before the source is opened,
/// so a missing or unreadable source does not turn an occupied destination
/// into an error. Filesystems without hard-link support fail with the
/// underlying OS error rather than falling back to an overwriting copy.
fn copy_absent_file(source: &Path, destination: &Path) -> Result<bool, Box<EvalAltResult>> {
    // Occupied includes dangling symlinks (`exists()` would miss those). Any
    // other lstat error, including ENOTDIR when a parent is a file, is not an
    // occupied name; parent creation and publication still report it.
    if std::fs::symlink_metadata(destination).is_ok() {
        return Ok(false);
    }
    ensure_parent(destination)?;
    let (staged, mut file) = create_staged_payload(destination)
        .map_err(|error| rhai_runtime_error(failed_to_write_path(destination, error)))?;
    use std::io::Write;
    let copy_result = (|| -> std::io::Result<()> {
        let mut source_file = std::fs::File::open(source)?;
        let permissions = source_file.metadata()?.permissions();
        std::io::copy(&mut source_file, &mut file)?;
        file.flush()?;
        file.set_permissions(permissions)?;
        Ok(())
    })();
    drop(file);
    if let Err(error) = copy_result {
        let _ = std::fs::remove_file(&staged);
        return Err(rhai_runtime_error(failed_to_read_path(source, error)));
    }
    publish_staged_no_clobber(&staged, destination)
}

fn publish_staged_no_clobber(
    staged: &Path,
    destination: &Path,
) -> Result<bool, Box<EvalAltResult>> {
    let published = match std::fs::hard_link(staged, destination) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(error) => {
            let _ = std::fs::remove_file(staged);
            return Err(rhai_runtime_error(failed_to_write_path(destination, error)));
        }
    };
    // The destination now names the complete payload; retire the staged name.
    // A leftover stage cannot make the destination partial, so a cleanup
    // failure here must not turn a successful publication into an error.
    let _ = std::fs::remove_file(staged);
    Ok(published)
}

fn create_staged_payload(destination: &Path) -> std::io::Result<(PathBuf, std::fs::File)> {
    for _ in 0..256 {
        let staged = staged_payload_path(destination);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
        {
            Ok(file) => return Ok((staged, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "failed to allocate a unique staged payload name",
    ))
}

fn staged_payload_path(destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "payload".to_owned());
    let nonce = STAGED_PAYLOAD_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    destination.with_file_name(format!(".{name}.effigy-publish-{pid}-{nanos}-{nonce}.tmp"))
}

fn list_recursive_paths(root: &Path, extension: Option<&str>) -> Result<Array, Box<EvalAltResult>> {
    // A missing or non-directory root is not an empty directory: a script that
    // mistyped a path must see the refusal instead of an empty list.
    let metadata = std::fs::metadata(root)
        .map_err(|error| rhai_runtime_error(failed_to_read_path(root, error)))?;
    if !metadata.is_dir() {
        return Err(rhai_runtime_error(format!(
            "{} is not a directory",
            root.display()
        )));
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

/// The entries a dotenv file declares.
///
/// `Ok(None)` means the file does not exist, which callers answer differently
/// (an empty map, an empty value, or `file_exists: false`). A file that exists
/// but cannot be read is an error, never an empty answer.
fn read_env_entries(path: &Path) -> Result<Option<BTreeMap<String, String>>, Box<EvalAltResult>> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|error| rhai_runtime_error(failed_to_read_path(path, error)))?;
    Ok(Some(parse_dotenv_entries(&contents)))
}

fn update_env_file_entry(path: &Path, key: &str, value: &str) -> Result<bool, Box<EvalAltResult>> {
    let rendered_entry = format!("{key}={}", render_dotenv_value(value));
    if !path.exists() {
        ensure_parent(path)?;
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
        if let Some(existing_key) = dotenv_line_key(raw_line) {
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
        if dotenv_line_key(raw_line).is_some_and(|existing_key| existing_key == key) {
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

/// The key one dotenv line declares, decoded by the shared dotenv parser so
/// this module keeps no second key grammar.
fn dotenv_line_key(line: &str) -> Option<String> {
    parse_dotenv_entries(line).into_keys().next()
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        copy_absent_file, dotenv_line_key, list_recursive_paths, publish_absent_payload,
        remove_env_file_entry, render_dotenv_value, staged_payload_path, update_env_file_entry,
    };

    fn fixture(name: &str) -> PathBuf {
        effigy_test_support::Fixture::process_lifetime(&format!("effigy-rhai-fs-{name}-"))
            .expect("fixture directory")
    }

    fn entries(path: &Path) -> Vec<String> {
        std::fs::read_to_string(path)
            .expect("env file")
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The writer quotes only what needs quoting, and the shared dotenv parser
    /// reads a bare value back unchanged.
    #[test]
    fn dotenv_values_are_rendered_for_the_shared_parser() {
        for (value, expected) in [
            ("plain", "plain"),
            ("with space", "\"with space\""),
            ("", "\"\""),
            ("a=b", "\"a=b\""),
        ] {
            assert_eq!(render_dotenv_value(value), expected, "{value}");
        }
    }

    /// One dotenv line's key comes from the shared parser, so `export`, spacing
    /// and quoted forms decode the same way the file loader decodes them.
    #[test]
    fn dotenv_line_keys_decode_through_the_shared_parser() {
        for (line, expected) in [
            ("KEY=value", Some("KEY")),
            ("export KEY = value", Some("KEY")),
            ("  SPACED=1  ", Some("SPACED")),
            ("# COMMENT=1", None),
            ("NO_EQUALS", None),
            ("", None),
        ] {
            assert_eq!(dotenv_line_key(line).as_deref(), expected, "{line}");
        }
    }

    /// Setting a key creates the file, appends a missing key, rewrites only the
    /// matching line, and reports an unchanged value without touching the file.
    #[test]
    fn env_file_set_creates_appends_updates_and_reports_unchanged() {
        let root = fixture("env-set");
        let path = root.join("nested/app.env");

        assert!(
            update_env_file_entry(&path, "APP_NAME", "Cumberland Local").expect("create"),
            "a missing file is created with the entry"
        );
        assert_eq!(entries(&path), ["APP_NAME=\"Cumberland Local\""]);

        std::fs::write(&path, "# keep this comment\nHOST=local\n").expect("write env");
        assert!(
            update_env_file_entry(&path, "APP_NAME", "Cumberland Local").expect("append"),
            "a missing key is appended"
        );
        assert_eq!(
            entries(&path),
            [
                "# keep this comment",
                "HOST=local",
                "APP_NAME=\"Cumberland Local\""
            ]
        );

        assert!(
            update_env_file_entry(&path, "HOST", "remote.test").expect("update"),
            "an existing key is rewritten"
        );
        assert_eq!(entries(&path)[1], "HOST=remote.test");

        assert!(
            !update_env_file_entry(&path, "HOST", "remote.test").expect("unchanged"),
            "the same value changes nothing"
        );
        assert_eq!(entries(&path)[1], "HOST=remote.test");
    }

    /// Removing a key drops only that line and keeps the rest of the file; a
    /// missing key or file reports no change.
    #[test]
    fn env_file_remove_drops_one_key_and_reports_no_change_otherwise() {
        let root = fixture("env-remove");
        let path = root.join("app.env");
        std::fs::write(&path, "# comment\nA=1\nexport B=2\n").expect("write env");

        assert!(remove_env_file_entry(&path, "B").expect("remove"));
        assert_eq!(entries(&path), ["# comment", "A=1"]);
        assert!(!remove_env_file_entry(&path, "B").expect("already absent"));
        assert!(!remove_env_file_entry(&root.join("missing.env"), "B").expect("missing file"));
    }

    /// No-clobber publication makes the payload visible once and refuses an
    /// occupied destination — including a symlink — without replacing it or
    /// leaving a staged name behind.
    #[test]
    fn publish_absent_payload_refuses_an_occupied_destination() {
        let root = fixture("publish");
        let destination = root.join("payload.txt");

        assert!(publish_absent_payload(&destination, b"first").expect("publish"));
        assert_eq!(std::fs::read(&destination).expect("read"), b"first");
        assert!(
            !publish_absent_payload(&destination, b"second").expect("refuse"),
            "an occupied destination is not replaced"
        );
        assert_eq!(std::fs::read(&destination).expect("read"), b"first");

        let link = root.join("link.txt");
        std::os::unix::fs::symlink(&destination, &link).expect("symlink");
        assert!(
            !publish_absent_payload(&link, b"third").expect("refuse symlink"),
            "a symlink occupies the name"
        );

        let leftovers: Vec<String> = std::fs::read_dir(&root)
            .expect("read fixture")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains("effigy-publish"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "no staged payload survives a publication: {leftovers:?}"
        );
    }

    /// A published copy owns its bytes: writing to the source afterwards leaves
    /// the destination untouched, and an occupied destination is refused before
    /// the source is opened.
    #[test]
    fn copy_absent_file_publishes_an_independent_copy() {
        let root = fixture("copy");
        let source = root.join("source.txt");
        let destination = root.join("copy.txt");
        std::fs::write(&source, "original\n").expect("write source");

        assert!(copy_absent_file(&source, &destination).expect("copy"));
        std::fs::write(&source, "changed\n").expect("rewrite source");
        assert_eq!(
            std::fs::read_to_string(&destination).expect("read copy"),
            "original\n",
            "the copy is independent of the source inode"
        );
        assert!(
            !copy_absent_file(&root.join("missing.txt"), &destination).expect("occupied"),
            "an occupied destination is refused before the source is read"
        );
    }

    /// Staged names stay in the destination's own directory and never repeat, so
    /// two publications in the same nanosecond cannot collide.
    #[test]
    fn staged_payload_names_stay_beside_the_destination_and_are_unique() {
        let destination = Path::new("/repo/dir/payload.txt");
        let first = staged_payload_path(destination);
        let second = staged_payload_path(destination);
        assert_ne!(first, second);
        assert_eq!(first.parent(), destination.parent());
        assert!(
            first
                .file_name()
                .expect("name")
                .to_string_lossy()
                .starts_with(".payload.txt.effigy-publish-"),
            "{}",
            first.display()
        );
    }

    /// A missing or non-directory root is a refusal, not an empty listing, and
    /// the extension filter keeps only files with that extension.
    #[test]
    fn list_recursive_paths_refuses_a_missing_root_and_filters_by_extension() {
        let root = fixture("list-recursive");
        std::fs::create_dir_all(root.join("nested")).expect("mkdir");
        std::fs::write(root.join("a.rs"), "fn a() {}\n").expect("write a");
        std::fs::write(root.join("nested/b.rs"), "fn b() {}\n").expect("write b");
        std::fs::write(root.join("nested/c.md"), "# c\n").expect("write c");

        let listed = list_recursive_paths(&root, Some(".rs")).expect("list");
        let listed: Vec<String> = listed.iter().map(ToString::to_string).collect();
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert!(
            listed.iter().all(|path| path.ends_with(".rs")),
            "{listed:?}"
        );

        assert!(
            list_recursive_paths(&root.join("missing"), None).is_err(),
            "a missing root is a refusal, not an empty directory"
        );
        assert!(
            list_recursive_paths(&root.join("a.rs"), None).is_err(),
            "a file is not a directory"
        );
    }
}
