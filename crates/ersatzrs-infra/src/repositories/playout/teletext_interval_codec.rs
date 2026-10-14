//! Private, versioned database framing; decoding never grants candidate authority.

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    validate_teletext_profile_values, TeletextInstallationSnapshot, TeletextPageContract,
    TeletextProfileCandidateSnapshot, TeletextProfileSetting, TeletextProfileValue,
    TeletextSecretReferenceKind,
};
use serde::{Deserialize, Serialize};

// Account for JSON's worst-case six-byte escaping plus fixed/key framing,
// without narrowing the domain's aggregate unresolved material budget.
pub(super) const MAX_BYTES: usize =
    6 * ersatzrs_domain::MAX_TELETEXT_OVERRIDE_SNAPSHOT_BYTES + 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u8,
    channel: i64,
    addon: String,
    profile: i64,
    revision: i64,
    contract: u32,
    installation: Installation,
    settings: Vec<Setting>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Installation {
    addon: String,
    revision: String,
    version: String,
    package: String,
    content: String,
    manifest: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Setting {
    key: String,
    kind: u8,
    value: String,
}

fn invalid() -> RepositoryError {
    RepositoryError::unavailable(
        "published teletext intervals",
        "candidate snapshot framing is invalid",
    )
}

pub(super) fn encode(candidate: &TeletextProfileCandidateSnapshot) -> Result<String> {
    let token = &candidate.installation;
    let snapshot = Snapshot {
        version: 1,
        channel: candidate.channel_id,
        addon: candidate.addon_id.clone(),
        profile: candidate.profile_id,
        revision: candidate.profile_revision,
        contract: candidate.contract.schema_version(),
        installation: Installation {
            addon: token.addon_id.clone(),
            revision: token.state_revision.clone(),
            version: token.version.clone(),
            package: token.package_sha256.clone(),
            content: token.content_sha256.clone(),
            manifest: token.manifest_sha256.clone(),
        },
        settings: candidate
            .effective_settings
            .iter()
            .map(|setting| {
                let (kind, value) = match &setting.value {
                    TeletextProfileValue::Literal(value) => (0, value),
                    TeletextProfileValue::SecretReference { kind, reference } => (
                        match kind {
                            TeletextSecretReferenceKind::Environment => 1,
                            TeletextSecretReferenceKind::File => 2,
                        },
                        reference,
                    ),
                };
                Setting {
                    key: setting.key.clone(),
                    kind,
                    value: value.clone(),
                }
            })
            .collect(),
    };
    let encoded = serde_json::to_string(&snapshot).map_err(|_| invalid())?;
    if encoded.len() > MAX_BYTES {
        return Err(invalid());
    }
    // The same bounded decoder checks the emitted private version/structure.
    decode(&encoded)?;
    Ok(encoded)
}

pub(super) fn decode(encoded: &str) -> Result<TeletextProfileCandidateSnapshot> {
    if encoded.is_empty() || encoded.len() > MAX_BYTES {
        return Err(invalid());
    }
    let snapshot: Snapshot = serde_json::from_str(encoded).map_err(|_| invalid())?;
    if snapshot.version != 1
        || snapshot.channel <= 0
        || snapshot.profile <= 0
        || snapshot.revision <= 0
        || snapshot.addon.is_empty()
        || snapshot.addon.len() > 256
        || snapshot.installation.addon != snapshot.addon
    {
        return Err(invalid());
    }
    let contract = match snapshot.contract {
        1 => TeletextPageContract::V1,
        2 => TeletextPageContract::V2,
        3 => TeletextPageContract::V3,
        _ => return Err(invalid()),
    };
    let settings = snapshot
        .settings
        .into_iter()
        .map(|setting| {
            let value = match setting.kind {
                0 => TeletextProfileValue::Literal(setting.value),
                1 | 2 => TeletextProfileValue::SecretReference {
                    kind: if setting.kind == 1 {
                        TeletextSecretReferenceKind::Environment
                    } else {
                        TeletextSecretReferenceKind::File
                    },
                    reference: setting.value,
                },
                _ => return Err(invalid()),
            };
            Ok(TeletextProfileSetting {
                key: setting.key,
                value,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    validate_teletext_profile_values("Published candidate", &settings).map_err(|_| invalid())?;
    let token = snapshot.installation;
    Ok(TeletextProfileCandidateSnapshot {
        channel_id: snapshot.channel,
        addon_id: snapshot.addon,
        profile_id: snapshot.profile,
        profile_revision: snapshot.revision,
        contract,
        installation: TeletextInstallationSnapshot {
            addon_id: token.addon,
            state_revision: token.revision,
            version: token.version,
            package_sha256: token.package,
            content_sha256: token.content,
            manifest_sha256: token.manifest,
        },
        effective_settings: settings,
    })
}
