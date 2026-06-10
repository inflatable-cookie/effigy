use sha2::{Digest, Sha256};

pub fn canonical_git_cache_identity(url: &str) -> String {
    let trimmed = url.trim();

    if let Some((host, path)) = trimmed.split_once(':') {
        if !host.contains('/') && !host.contains('\\') && host.contains('@') {
            let host = host.split_once('@').map_or(host, |(_, rest)| rest);
            return format!(
                "{}/{}",
                host.to_ascii_lowercase(),
                normalize_git_repo_path(path)
            );
        }
    }

    if let Some((scheme, rest)) = trimmed.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        if scheme == "ssh" || scheme == "git" || scheme == "https" || scheme == "http" {
            if let Some((authority, path)) = rest.split_once('/') {
                let authority = authority
                    .rsplit_once('@')
                    .map_or(authority, |(_, host)| host);
                let host = authority
                    .split_once(':')
                    .map_or(authority, |(host, _)| host);
                return format!(
                    "{}/{}",
                    host.to_ascii_lowercase(),
                    normalize_git_repo_path(path)
                );
            }
        }
    }

    if let Some(path) = trimmed.strip_prefix("file://") {
        return format!("local/{}", normalize_local_git_path(path));
    }

    format!("local/{}", normalize_local_git_path(trimmed))
}

pub fn sanitize_cache_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '_',
            _ => ch,
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "_".to_string()
    } else {
        sanitized
    }
}

pub fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    hex::encode(hasher.finalize())
}

fn normalize_git_repo_path(path: &str) -> String {
    path.trim_matches('/')
        .trim_end_matches(".git")
        .to_ascii_lowercase()
}

fn normalize_local_git_path(path: &str) -> String {
    path.trim_end_matches('/')
        .trim_end_matches(".git")
        .replace('\\', "/")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{canonical_git_cache_identity, sanitize_cache_segment, sha256_hex};

    #[test]
    fn canonical_git_cache_identity_normalizes_common_remote_forms() {
        let ssh = canonical_git_cache_identity("git@github.com:Acme/Bundle.git");
        let https = canonical_git_cache_identity("https://github.com/acme/bundle.git");
        let ssh_scheme = canonical_git_cache_identity("ssh://git@github.com/ACME/bundle");

        assert_eq!(ssh, "github.com/acme/bundle");
        assert_eq!(ssh, https);
        assert_eq!(https, ssh_scheme);
    }

    #[test]
    fn canonical_git_cache_identity_normalizes_local_paths() {
        assert_eq!(
            canonical_git_cache_identity("file:///Users/Tom/Repo.git"),
            "local//users/tom/repo"
        );
        assert_eq!(
            canonical_git_cache_identity("/Users/Tom/Repo/"),
            "local//users/tom/repo"
        );
    }

    #[test]
    fn sanitize_cache_segment_replaces_path_delimiters() {
        assert_eq!(sanitize_cache_segment("refs/heads/main"), "refs_heads_main");
        assert_eq!(sanitize_cache_segment("host:path\\leaf"), "host_path_leaf");
        assert_eq!(sanitize_cache_segment(""), "_");
    }

    #[test]
    fn sha256_hex_is_stable() {
        assert_eq!(
            sha256_hex(b"github.com/acme/bundle"),
            "fd5f7a504d36a805f75e306d4a9777259904b271f7a648baa1973420a97f782b"
        );
    }
}
