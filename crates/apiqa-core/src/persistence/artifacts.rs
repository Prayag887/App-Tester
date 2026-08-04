use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRef {
    pub sha256: String,
    pub path: PathBuf,
    pub media_type: String,
    pub redacted: bool,
}
#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("artifact I/O failed: {0}")]
    Io(#[from] std::io::Error),
}
pub fn store(
    root: &Path,
    bytes: &[u8],
    media_type: &str,
    redacted: bool,
) -> Result<ArtifactRef, ArtifactError> {
    let sha256 = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>();
    let path = root.join(&sha256);
    fs::create_dir_all(root)?;
    if !path.exists() {
        fs::write(&path, bytes)?;
    }
    Ok(ArtifactRef {
        sha256,
        path,
        media_type: media_type.to_owned(),
        redacted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_bytes_under_their_sha256_name_and_returns_a_ref() {
        let root =
            std::env::temp_dir().join(format!("app-tester-artifacts-{}", uuid::Uuid::new_v4()));
        let artifact = store(&root, b"hello artifact", "application/json", true).unwrap();
        assert_eq!(artifact.sha256.len(), 64);
        assert_eq!(artifact.path, root.join(&artifact.sha256));
        assert_eq!(artifact.media_type, "application/json");
        assert!(artifact.redacted);
        assert_eq!(std::fs::read(&artifact.path).unwrap(), b"hello artifact");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deduplicates_identical_bytes() {
        let root =
            std::env::temp_dir().join(format!("app-tester-artifacts-{}", uuid::Uuid::new_v4()));
        let first = store(&root, b"same", "text/plain", false).unwrap();
        let second = store(&root, b"same", "text/plain", false).unwrap();
        assert_eq!(first.path, second.path);
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            1,
            "one file per content"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn different_content_gets_a_different_path() {
        let root =
            std::env::temp_dir().join(format!("app-tester-artifacts-{}", uuid::Uuid::new_v4()));
        let first = store(&root, b"one", "text/plain", false).unwrap();
        let second = store(&root, b"two", "text/plain", false).unwrap();
        assert_ne!(first.sha256, second.sha256);
        assert_ne!(first.path, second.path);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn error_surfaces_io_failures() {
        // A path whose parent is a file cannot be created as a directory.
        let broken =
            std::env::temp_dir().join(format!("app-tester-artifacts-{}", uuid::Uuid::new_v4()));
        std::fs::write(&broken, b"not a directory").unwrap();
        assert!(store(&broken, b"bytes", "text/plain", false).is_err());
        let _ = std::fs::remove_file(&broken);
    }
}
