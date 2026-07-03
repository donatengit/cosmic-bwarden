use crate::error::{Error, Result};
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
use std::path::PathBuf;

pub fn set_config_override(path: PathBuf) {
    std::env::set_var("COSMIC_BWARDEN_CONFIG", path);
}

pub fn set_socket_override(path: PathBuf) {
    std::env::set_var("COSMIC_BWARDEN_SOCKET", path);
}

pub fn set_ssh_socket_override(path: PathBuf) {
    std::env::set_var("COSMIC_BWARDEN_SSH_SOCKET", path);
}

pub fn make_all() -> Result<()> {
    create_dir_all_with_permissions(&cache_dir(), 0o700)?;
    create_dir_all_with_permissions(&runtime_dir(), 0o700)?;
    create_dir_all_with_permissions(&data_dir(), 0o700)?;

    Ok(())
}

fn create_dir_all_with_permissions(
    path: &std::path::Path,
    mode: u32,
) -> Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(mode)
        .create(path)
        .map_err(|source| Error::Other(format!("failed to create directory {}: {}", path.display(), source)))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|source| Error::Other(format!("failed to set permissions on {}: {}", path.display(), source)))?;
    Ok(())
}

pub fn config_file() -> std::path::PathBuf {
    std::env::var_os("COSMIC_BWARDEN_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| config_dir().join("config.json"))
}

const INVALID_PATH: &percent_encoding::AsciiSet =
    &percent_encoding::CONTROLS.add(b'/').add(b'%').add(b':');

pub fn db_file(server: &str, email: &str) -> std::path::PathBuf {
    let server =
        percent_encoding::percent_encode(server.as_bytes(), INVALID_PATH)
            .to_string();
    // Encode the email too — an address containing '/' or ".." must not be able
    // to steer the cache path outside the cache dir.
    let email =
        percent_encoding::percent_encode(email.as_bytes(), INVALID_PATH)
            .to_string();
    cache_dir().join(format!("{server}:{email}.json"))
}

pub fn device_id_file() -> std::path::PathBuf {
    data_dir().join("device_id")
}

/// Path for the per-account TPM sealed-blob file.
/// Uses an 8-hex-char prefix of SHA-256(server ‖ '\0' ‖ email) to avoid
/// special characters in the filename while keeping it unique per account.
pub fn tpm_blob_file(server: &str, email: &str) -> std::path::PathBuf {
    use sha2::{Digest as _, Sha256};
    let mut h = Sha256::new();
    h.update(server.as_bytes());
    h.update(b"\0");
    h.update(email.as_bytes());
    let hex = format!("{:x}", h.finalize());
    data_dir().join(format!("tpm_sealed_{}.bin", &hex[..16]))
}

/// Path for the per-account TPM sealed master-password-hash blob file.
/// Distinct from `tpm_blob_file` (vault keys) — same account-hash prefix but
/// different filename so both blobs can coexist independently.
pub fn tpm_hash_blob_file(server: &str, email: &str) -> std::path::PathBuf {
    use sha2::{Digest as _, Sha256};
    let mut h = Sha256::new();
    h.update(server.as_bytes());
    h.update(b"\0");
    h.update(email.as_bytes());
    let hex = format!("{:x}", h.finalize());
    data_dir().join(format!("tpm_sealed_hash_{}.bin", &hex[..16]))
}

pub fn socket_file() -> std::path::PathBuf {
    std::env::var_os("COSMIC_BWARDEN_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime_dir().join("socket"))
}

pub fn ssh_agent_socket_file() -> std::path::PathBuf {
    std::env::var_os("COSMIC_BWARDEN_SSH_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime_dir().join("ssh-agent-socket"))
}

fn config_dir() -> std::path::PathBuf {
    let project_dirs =
        directories::ProjectDirs::from("", "", &profile()).unwrap();
    project_dirs.config_dir().to_path_buf()
}

pub fn cache_dir() -> std::path::PathBuf {
    let project_dirs =
        directories::ProjectDirs::from("", "", &profile()).unwrap();
    project_dirs.cache_dir().to_path_buf()
}

fn data_dir() -> std::path::PathBuf {
    let project_dirs =
        directories::ProjectDirs::from("", "", &profile()).unwrap();
    project_dirs.data_dir().to_path_buf()
}

fn runtime_dir() -> std::path::PathBuf {
    let project_dirs =
        directories::ProjectDirs::from("", "", &profile()).unwrap();
    project_dirs.runtime_dir().map_or_else(
        || {
            format!(
                "{}/{}-{}",
                std::env::temp_dir().to_string_lossy(),
                &profile(),
                rustix::process::getuid().as_raw()
            )
            .into()
        },
        std::path::Path::to_path_buf,
    )
}

pub fn profile() -> String {
    match std::env::var("COSMIC_BWARDEN_PROFILE") {
        Ok(profile) if !profile.is_empty() => format!("cosmic-bwarden-{profile}"),
        _ => "cosmic-bwarden".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A server/email containing path separators or traversal sequences must not
    // steer the DB path outside the cache dir: the result must have the same
    // number of path components as a benign input (i.e. exactly one filename is
    // appended), with `/` percent-encoded rather than creating subdirectories.
    #[test]
    fn db_file_encodes_separators_preventing_traversal() {
        let benign = db_file("https://vault.example", "user@example.com");
        let evil = db_file("https://vault.example", "../../../../etc/passwd");

        assert_eq!(
            benign.components().count(),
            evil.components().count(),
            "malicious email must not add path components"
        );

        let name = evil.file_name().unwrap().to_str().unwrap();
        assert!(name.contains("%2F"), "'/' must be percent-encoded: {name}");
        assert!(
            !evil.components().any(|c| c.as_os_str() == ".."),
            "no `..` component may appear in the path: {evil:?}"
        );
    }

    // ':' is both an INVALID_PATH char and the server/email delimiter, so it must
    // be encoded within each field to avoid ambiguity and path issues.
    #[test]
    fn db_file_encodes_colon_in_fields() {
        let p = db_file("host:8080", "a:b");
        let name = p.file_name().unwrap().to_str().unwrap();
        assert!(name.contains("host%3A8080"), "colon in server must be encoded: {name}");
        assert!(name.contains("a%3Ab"), "colon in email must be encoded: {name}");
    }

    // '%' must itself be encoded, otherwise a crafted value could forge an encoded
    // separator (e.g. a literal "%2F" in the input becoming a decoded '/').
    #[test]
    fn db_file_encodes_percent() {
        let p = db_file("s", "a%2Fb");
        let name = p.file_name().unwrap().to_str().unwrap();
        assert!(name.contains("a%252Fb"), "literal '%' must be encoded: {name}");
    }
}
