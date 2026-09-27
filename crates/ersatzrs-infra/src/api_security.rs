//! Persisted machine API credential, corresponding to upstream
//! `CreateApiKeyService` and `ApiKeyAuthenticationHandler` (ADR 0103).

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq as _;

const MAX_DOCUMENT_BYTES: u64 = 65_536;

/// A machine credential that can be checked but never formatted or serialized.
#[derive(Clone)]
pub struct ApiKey(Arc<str>);

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey([REDACTED])")
    }
}

impl ApiKey {
    /// Compare the complete header value in constant time for equal lengths.
    #[must_use]
    pub fn verifies(&self, supplied: &[u8]) -> bool {
        self.0.as_bytes().ct_eq(supplied).into()
    }
}

/// Startup failures contain no credential, document excerpt or private path.
#[derive(Debug, thiserror::Error)]
pub enum ApiKeyError {
    #[error("API credential storage is unavailable")]
    Storage,
    #[error("another process is initializing the API credential; retry startup")]
    Busy,
    #[error("API credential document exceeds the size limit")]
    TooLarge,
    #[error("operating-system randomness is unavailable")]
    Randomness,
}

#[derive(Deserialize, Serialize)]
struct Secrets {
    #[serde(rename = "ApiKey")]
    api_key: String,
}

/// Load or initialize the upstream-compatible `api-secrets.json` document.
///
/// Call from blocking startup work, using the durable application state root.
/// A nonblocking file lock serializes initialization across processes; a busy
/// caller must fail closed or retry instead of choosing another credential.
/// Existing nonblank printable ASCII keys are preserved byte-for-byte.
///
/// # Errors
/// Returns a redacted error on storage, lock or OS-randomness failure. Invalid
/// JSON or an empty key is replaced atomically, matching upstream startup.
pub fn load_or_create(state_root: &Path) -> Result<ApiKey, ApiKeyError> {
    fs::create_dir_all(state_root).map_err(|_| ApiKeyError::Storage)?;
    let lock_path = state_root.join("api-secrets.lock");
    reject_non_file(&lock_path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    // Unix permissions apply from creation; Windows uses the state directory's
    // inherited ACL, as for the other durable application credentials.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let lock = options.open(lock_path).map_err(|_| ApiKeyError::Storage)?;
    match fs4::FileExt::try_lock(&lock) {
        Ok(()) => {}
        Err(fs4::TryLockError::WouldBlock) => return Err(ApiKeyError::Busy),
        Err(_) => return Err(ApiKeyError::Storage),
    }
    let path = state_root.join("api-secrets.json");
    reject_non_file(&path)?;
    if let Some(key) = read_key(&path)? {
        return Ok(key);
    }
    let mut random = [0; 32];
    getrandom::fill(&mut random).map_err(|_| ApiKeyError::Randomness)?;
    let document = Secrets {
        api_key: URL_SAFE_NO_PAD.encode(random),
    };
    // NamedTempFile creates private files on Unix. On Windows it inherits the
    // same directory ACL as the final file. Persist replaces atomically while
    // the sidecar lock prevents cooperating initializers from rotating a key.
    let mut temporary =
        tempfile::NamedTempFile::new_in(state_root).map_err(|_| ApiKeyError::Storage)?;
    serde_json::to_writer(temporary.as_file_mut(), &document).map_err(|_| ApiKeyError::Storage)?;
    temporary.flush().map_err(|_| ApiKeyError::Storage)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| ApiKeyError::Storage)?;
    temporary.persist(&path).map_err(|_| ApiKeyError::Storage)?;
    Ok(ApiKey(document.api_key.into()))
}

fn reject_non_file(path: &Path) -> Result<(), ApiKeyError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) | Err(_) => Err(ApiKeyError::Storage),
    }
}

fn read_key(path: &Path) -> Result<Option<ApiKey>, ApiKeyError> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(ApiKeyError::Storage),
    };
    let mut bytes = Vec::new();
    file.take(MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ApiKeyError::Storage)?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(ApiKeyError::TooLarge);
    }
    Ok(serde_json::from_slice::<Secrets>(&bytes)
        .ok()
        .filter(|document| {
            !document.api_key.trim().is_empty()
                && document
                    .api_key
                    .bytes()
                    .all(|byte| (32..=126).contains(&byte))
        })
        .map(|document| ApiKey(document.api_key.into())))
}

#[cfg(test)]
mod tests;
