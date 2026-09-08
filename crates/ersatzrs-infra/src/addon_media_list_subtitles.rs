//! Explicit global-language acquisition for provider-owned manual subtitles.

use crate::{
    db::{Db, DbPool},
    repositories::SqlxResultExt as _,
};
use ersatzrs_core::{language_codes, repositories::Result};
use ersatzrs_domain::{
    CONFIG_KEY_FFMPEG_FALLBACK_SUBTITLE_LANGUAGE_CODE,
    CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE,
};
use sha2::{Digest as _, Sha256};
use sqlx::Row as _;
use sqlx::Transaction;
use tokio::io::AsyncReadExt as _;

/// Verified immutable manual-caption bytes. The caller binds the artifact ID
/// to the selected publication, rather than mutable current item ownership.
pub struct VerifiedSubtitleArtifact {
    pub content: String,
    pub language: String,
    pub prepared: bool,
}

/// Read one bounded Ready subtitle object after checking its digest and path.
///
/// # Errors
/// Rejects missing, changed, oversized or unconfined artifacts.
pub async fn read_verified_subtitle_artifact(
    pool: &DbPool,
    list_id: i64,
    artifact_id: i64,
    root: &std::path::Path,
) -> Result<VerifiedSubtitleArtifact> {
    let invalid = || {
        ersatzrs_core::repositories::RepositoryError::Conflict(
            "the published subtitle artifact is unavailable".into(),
        )
    };
    let row = sqlx::query(r#"SELECT "RelativePath","ByteLength","Sha256","SubtitleLanguage","SubtitleTimeline"
        FROM "AddonMediaListArtifact" WHERE "Id"=? AND "AddonMediaListId"=? AND "Kind"=0 AND "State"=2
          AND "CompletedAt" IS NOT NULL"#)
        .bind(artifact_id).bind(list_id).fetch_optional(pool).await.repository()?.ok_or_else(invalid)?;
    let length = row
        .try_get::<Option<i64>, _>("ByteLength")
        .repository()?
        .ok_or_else(invalid)?;
    let limit = crate::addon_media_list_subtitle_artifacts::MAXIMUM_SUBTITLE_FILE_BYTES;
    if length <= 0 || u64::try_from(length).map_err(|_| invalid())? > limit {
        return Err(invalid());
    }
    let relative: String = row
        .try_get::<Option<String>, _>("RelativePath")
        .repository()?
        .ok_or_else(invalid)?;
    let path = crate::addon_media_list_artifacts::confined_file(root, &relative).await?;
    let mut bytes = Vec::new();
    tokio::fs::File::open(path)
        .await
        .map_err(|_| invalid())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| invalid())?;
    let digest =
        Sha256::digest(&bytes)
            .iter()
            .fold(String::with_capacity(64), |mut output, byte| {
                use std::fmt::Write as _;
                write!(output, "{byte:02x}").expect("writing to a string");
                output
            });
    if i64::try_from(bytes.len()).map_err(|_| invalid())? != length
        || row
            .try_get::<Option<String>, _>("Sha256")
            .repository()?
            .as_deref()
            != Some(digest.as_str())
    {
        return Err(invalid());
    }
    let prepared = match row
        .try_get::<Option<i32>, _>("SubtitleTimeline")
        .repository()?
    {
        Some(0) => false,
        Some(1) => true,
        _ => return Err(invalid()),
    };
    Ok(VerifiedSubtitleArtifact {
        content: String::from_utf8(bytes).map_err(|_| invalid())?,
        language: row
            .try_get::<Option<String>, _>("SubtitleLanguage")
            .repository()?
            .ok_or_else(invalid)?,
        prepared,
    })
}

/// Read both preferences in the caller's transaction so job completion cannot
/// race a settings change. Unset/undefined is not a request for every language.
pub(crate) async fn requested_languages_in_tx(tx: &mut Transaction<'_, Db>) -> Result<Vec<String>> {
    let mut configured = Vec::new();
    for key in [
        CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE,
        CONFIG_KEY_FFMPEG_FALLBACK_SUBTITLE_LANGUAGE_CODE,
    ] {
        let value: Option<String> =
            sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"=?"#)
                .bind(key)
                .fetch_optional(&mut **tx)
                .await
                .repository()?
                .flatten();
        configured.push(value);
    }
    Ok(provider_languages(
        configured.iter().filter_map(Option::as_deref),
    ))
}

pub(crate) async fn requested_languages(pool: &DbPool) -> Result<Vec<String>> {
    // Read-only transaction: read primary and fallback language settings consistently.
    let mut tx = pool.begin().await.repository()?;
    let languages = requested_languages_in_tx(&mut tx).await?;
    tx.commit().await.repository()?;
    Ok(languages)
}

fn provider_languages<'a>(configured: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut result = Vec::new();
    for value in configured {
        let value = value.trim().to_ascii_lowercase();
        if value.is_empty() || matches!(value.as_str(), "und" | "zxx" | "mul" | "all") {
            continue;
        }
        let aliases = language_codes::all_language_codes([&value]);
        let language = aliases
            .into_iter()
            .find(|alias| alias.len() == 2)
            .unwrap_or(value);
        let primary = language.split('-').next().unwrap_or_default();
        if !result
            .iter()
            .any(|existing: &String| existing.split('-').next() == Some(primary))
        {
            result.push(language);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquisition_languages_are_explicit_ordered_and_alias_deduplicated() {
        assert_eq!(provider_languages(["nld", "eng"]), ["nl", "en"]);
        assert_eq!(provider_languages(["dut", "nl"]), ["nl"]);
        assert_eq!(provider_languages(["", "und"]), Vec::<String>::new());
        assert_eq!(provider_languages(["", "eng"]), ["en"]);
        assert_eq!(provider_languages(["en-GB", "en"]), ["en-gb"]);
    }
}
