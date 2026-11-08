//! ADR0131 admission for the explicitly negotiated reader, not a runtime pin.
use std::{
    fmt::Write as _,
    io::Read as _,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::{io::AsyncReadExt as _, process::Command};

const OUTPUT_LIMIT: usize = 128 * 1024;
const BINARY_LIMIT: u64 = 128 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
const LAYOUT: &str = super::publication::indexed::INDEX_VERSION;

/// Redacted failures never include describe output, private paths or arguments.
#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum IndexedRuntimeError {
    #[error("Next does not advertise the required indexed playout contract")]
    Contract,
    #[error("Next indexed reader identity has changed")]
    Identity,
    #[error("Next indexed reader describe probe failed or exceeded its budget")]
    Probe,
}

/// Proof bound to one executable's bytes. Only a successful describe probe can
/// create a production permit; a version string is insufficient.
#[derive(Clone)]
pub struct IndexedReaderPermit {
    binary: PathBuf,
    digest: [u8; 32],
}

impl std::fmt::Debug for IndexedReaderPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IndexedReaderPermit { identity: verified }")
    }
}

#[derive(Deserialize)]
struct Description {
    describe_version: u32,
    contract: Contract,
}

#[derive(Deserialize)]
struct Contract {
    channel_config_schema: Schema,
    playout_schema: Schema,
    playout_layouts: Vec<String>,
    output_files: OutputFiles,
}

#[derive(Deserialize)]
struct OutputFiles {
    playlist: String,
    subtitle_playlist: String,
    ready: String,
    heartbeat: String,
}

#[derive(Deserialize)]
struct Schema {
    uri: String,
    breaking: u32,
    compatible: u32,
}

impl Schema {
    fn supports(&self, kind: &str, breaking: u32, compatible: u32) -> bool {
        self.breaking == breaking
            && self.compatible >= compatible
            && self.uri
                == format!(
                    "https://ersatztv.org/{kind}/version/0.{}.{}",
                    self.breaking, self.compatible
                )
    }
}

fn validate(output: &[u8]) -> Result<(), IndexedRuntimeError> {
    if output.len() > OUTPUT_LIMIT {
        return Err(IndexedRuntimeError::Contract);
    }
    let description: Description =
        serde_json::from_slice(output).map_err(|_| IndexedRuntimeError::Contract)?;
    let files = &description.contract.output_files;
    if files.playlist != "live.m3u8"
        || files.subtitle_playlist != "live_sub.m3u8"
        || files.ready != ".ready"
        || files.heartbeat != ".heartbeat"
    {
        return Err(IndexedRuntimeError::Contract);
    }
    if description.describe_version != 1
        || !description
            .contract
            .channel_config_schema
            .supports("channel", 1, 4)
        || !description
            .contract
            .playout_schema
            .supports("playout", 0, 7)
        || !description
            .contract
            .playout_layouts
            .iter()
            .any(|layout| layout == LAYOUT)
    {
        return Err(IndexedRuntimeError::Contract);
    }
    Ok(())
}

async fn identity(binary: &Path) -> Result<[u8; 32], IndexedRuntimeError> {
    let binary = binary.to_owned();
    tokio::task::spawn_blocking(move || {
        // Reject nonregular paths before opening: a FIFO can otherwise wait
        // forever before the bounded hashing loop even starts.
        let before = std::fs::metadata(&binary).map_err(|_| IndexedRuntimeError::Identity)?;
        if !before.is_file() || before.len() == 0 || before.len() > BINARY_LIMIT {
            return Err(IndexedRuntimeError::Identity);
        }
        let mut file = std::fs::File::open(binary).map_err(|_| IndexedRuntimeError::Identity)?;
        let meta = file.metadata().map_err(|_| IndexedRuntimeError::Identity)?;
        if !meta.is_file() || meta.len() == 0 || meta.len() > BINARY_LIMIT {
            return Err(IndexedRuntimeError::Identity);
        }
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];
        let mut total = 0u64;
        loop {
            let length = file
                .read(&mut buffer)
                .map_err(|_| IndexedRuntimeError::Identity)?;
            if length == 0 {
                break;
            }
            total += length as u64;
            if total > BINARY_LIMIT {
                return Err(IndexedRuntimeError::Identity);
            }
            hasher.update(&buffer[..length]);
        }
        Ok(hasher.finalize().into())
    })
    .await
    .map_err(|_| IndexedRuntimeError::Identity)?
}

impl IndexedReaderPermit {
    pub(super) async fn qualify(
        binary: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<Self, IndexedRuntimeError> {
        Self::probe(binary, TIMEOUT, expected_sha256).await
    }

    async fn probe(
        binary: &Path,
        timeout: Duration,
        expected_sha256: Option<&str>,
    ) -> Result<Self, IndexedRuntimeError> {
        let binary = tokio::fs::canonicalize(binary)
            .await
            .map_err(|_| IndexedRuntimeError::Identity)?;
        let binary = binary.as_path();
        let digest = identity(binary).await?;
        if let Some(expected) = expected_sha256 {
            let mut actual = String::with_capacity(64);
            for byte in digest {
                write!(&mut actual, "{byte:02x}").map_err(|_| IndexedRuntimeError::Identity)?;
            }
            if actual != expected {
                return Err(IndexedRuntimeError::Identity);
            }
        }
        let mut command = Command::new(binary);
        command
            .arg("describe")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = crate::addons::spawn_managed_addon_process(command)
            .map_err(|_| IndexedRuntimeError::Probe)?;
        let stdout = child.stdout().take().ok_or(IndexedRuntimeError::Probe)?;
        let mut output = Vec::new();
        let result = tokio::time::timeout(timeout, async {
            stdout
                .take(OUTPUT_LIMIT as u64 + 1)
                .read_to_end(&mut output)
                .await
                .map_err(|_| IndexedRuntimeError::Probe)?;
            if output.len() > OUTPUT_LIMIT {
                return Err(IndexedRuntimeError::Probe);
            }
            let status = child.wait().await.map_err(|_| IndexedRuntimeError::Probe)?;
            if !status.success() {
                return Err(IndexedRuntimeError::Probe);
            }
            validate(&output)
        })
        .await;
        match result {
            Ok(Ok(())) => {}
            other => {
                let _ = tokio::time::timeout(TIMEOUT, child.kill()).await;
                return Err(other
                    .ok()
                    .and_then(Result::err)
                    .unwrap_or(IndexedRuntimeError::Probe));
            }
        }
        if identity(binary).await? != digest {
            return Err(IndexedRuntimeError::Identity);
        }
        Ok(Self {
            binary: binary.to_owned(),
            digest,
        })
    }

    /// Recheck the requested executable before indexed session admission.
    /// # Errors
    /// Rejects a different path, changed bytes or an unavailable executable.
    pub async fn authorize(&self, binary: &Path) -> Result<(), IndexedRuntimeError> {
        let binary = tokio::fs::canonicalize(binary)
            .await
            .map_err(|_| IndexedRuntimeError::Identity)?;
        if binary != self.binary || identity(&binary).await? != self.digest {
            return Err(IndexedRuntimeError::Identity);
        }
        Ok(())
    }

    pub(super) async fn health(&self) -> Result<(), IndexedRuntimeError> {
        self.authorize(&self.binary).await
    }

    #[cfg(test)]
    pub(crate) fn fixture() -> Self {
        Self {
            binary: PathBuf::from("synthetic-indexed-reader"),
            digest: [0; 32],
        }
    }
}

#[cfg(test)]
#[path = "indexed_runtime_tests.rs"]
pub(in crate::next_engine) mod tests;
