use std::path::{Path, PathBuf};

pub(crate) fn looks_like_unprefixed_registry_ref(value: &str) -> bool {
    let Some((registry, rest)) = value.split_once('/') else {
        return false;
    };
    let registry_like = registry.contains('.') || registry.contains(':') || registry == "localhost";
    if !registry_like {
        return false;
    }
    rest.contains(':') || rest.contains("@sha256:")
}

pub(crate) fn resolve_local_path(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

pub(crate) fn staging_dir_name(path: &Path) -> String {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy())
        .unwrap_or_else(|| "artifact".into());
    let slug = slugify(&file_name);
    format!(
        "{slug}-{:016x}",
        stable_hash(path.to_string_lossy().as_ref())
    )
}

pub(crate) fn path_relative_to_root(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

pub(crate) fn safe_oci_pull_dir_name(reference: &str) -> String {
    let mut slug = String::new();
    for ch in reference.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "artifact".to_owned()
    } else {
        slug.to_owned()
    }
}

pub(crate) fn digest_from_ref(reference: &str) -> Option<String> {
    let (_, digest) = reference.split_once('@')?;
    Some(digest.to_owned())
}

pub(crate) fn redact_oci_reference(reference: &str) -> String {
    let Some((authority, rest)) = reference.split_once('/') else {
        return reference.to_owned();
    };
    let Some((_, host)) = authority.rsplit_once('@') else {
        return reference.to_owned();
    };
    format!("***@{host}/{rest}")
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "artifact".to_owned()
    } else {
        slug.to_owned()
    }
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
