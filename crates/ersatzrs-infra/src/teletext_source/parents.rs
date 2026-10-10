use ersatzrs_core::repositories::{ManagedLibraryOperationClaim, Result};
use ersatzrs_domain::{MediaSourceKind, SourceTeletextBinding, SourceTeletextObservation};
use sqlx::Transaction;

use super::{column, invalid, storage, SourceTeletextParents};
use crate::db::Db;

pub(super) struct Context {
    library_id: i64,
}

pub(super) async fn validate(
    tx: &mut Transaction<'_, Db>,
    parents: SourceTeletextParents,
    observation: &SourceTeletextObservation,
) -> Result<Context> {
    if [
        parents.media_version_id,
        parents.media_stream_id,
        parents.media_file_id,
    ]
    .iter()
    .any(|id| *id <= 0)
    {
        return Err(invalid("parent identity"));
    }
    let row = sqlx::query(
        r#"SELECT v."MovieId", v."EpisodeId", v."MusicVideoId", v."OtherVideoId",
            v."SongId", v."ImageId", v."RemoteStreamId", s."Index", s."MediaStreamKind",
            CASE WHEN s."Codec"='dvb_teletext' THEN 1 ELSE 0 END AS "TeletextCodec"
        FROM "MediaVersion" v JOIN "MediaStream" s ON s."MediaVersionId"=v."Id" AND s."Id"=?
        JOIN "MediaFile" f ON f."MediaVersionId"=v."Id" AND f."Id"=? WHERE v."Id"=?"#,
    )
    .bind(parents.media_stream_id)
    .bind(parents.media_file_id)
    .bind(parents.media_version_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| storage())?
    .ok_or_else(|| invalid("current parent association"))?;
    if column::<i64>(&row, "Index")? != i64::from(observation.stream_index)
        || column::<i64>(&row, "MediaStreamKind")? != 3
        || column::<i64>(&row, "TeletextCodec")? != 1
    {
        return Err(invalid("current teletext stream"));
    }
    let owner = single_owner(&row)?;
    let context = sqlx::query(
        r#"SELECT l."Id" AS "LibraryId", l."MediaSourceId",
            EXISTS(SELECT 1 FROM "LocalLibrary" WHERE "Id"=l."Id") AS "LocalLibrary",
            EXISTS(SELECT 1 FROM "PlexLibrary" WHERE "Id"=l."Id") AS "PlexLibrary",
            EXISTS(SELECT 1 FROM "JellyfinLibrary" WHERE "Id"=l."Id") AS "JellyfinLibrary",
            EXISTS(SELECT 1 FROM "EmbyLibrary" WHERE "Id"=l."Id") AS "EmbyLibrary",
            EXISTS(SELECT 1 FROM "LocalMediaSource" WHERE "Id"=s."Id") AS "LocalSource",
            EXISTS(SELECT 1 FROM "PlexMediaSource" WHERE "Id"=s."Id") AS "PlexSource",
            EXISTS(SELECT 1 FROM "JellyfinMediaSource" WHERE "Id"=s."Id") AS "JellyfinSource",
            EXISTS(SELECT 1 FROM "EmbyMediaSource" WHERE "Id"=s."Id") AS "EmbySource"
        FROM "MediaItem" m JOIN "LibraryPath" p ON p."Id"=m."LibraryPathId"
        JOIN "Library" l ON l."Id"=p."LibraryId"
        JOIN "MediaSource" s ON s."Id"=l."MediaSourceId" WHERE m."Id"=?"#,
    )
    .bind(owner)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| storage())?
    .ok_or_else(|| invalid("owner source association"))?;
    let library_kind = single_kind(
        &context,
        [
            "LocalLibrary",
            "PlexLibrary",
            "JellyfinLibrary",
            "EmbyLibrary",
        ],
    )?;
    let source_kind = single_kind(
        &context,
        ["LocalSource", "PlexSource", "JellyfinSource", "EmbySource"],
    )?;
    if library_kind != source_kind {
        return Err(invalid("library source kind"));
    }
    if let SourceTeletextBinding::Bound(binding) = &observation.binding {
        if binding.media_version_id != parents.media_version_id
            || binding.media_stream_id != parents.media_stream_id
            || binding.media_file_id != parents.media_file_id
            || binding.media_source_id != column::<i64>(&context, "MediaSourceId")?
            || binding.provider != source_kind
        {
            return Err(invalid("bound source association"));
        }
    }
    Ok(Context {
        library_id: column(&context, "LibraryId")?,
    })
}

fn single_owner(row: &crate::db::DbRow) -> Result<i64> {
    // No COALESCE: two populated columns are ambiguous even with the same ID.
    let mut owner = None;
    for name in [
        "MovieId",
        "EpisodeId",
        "MusicVideoId",
        "OtherVideoId",
        "SongId",
        "ImageId",
        "RemoteStreamId",
    ] {
        if let Some(id) = column::<Option<i64>>(row, name)? {
            if owner.replace(id).is_some() {
                return Err(invalid("ambiguous version owner"));
            }
        }
    }
    owner.ok_or_else(|| invalid("missing version owner"))
}

fn single_kind(row: &crate::db::DbRow, names: [&str; 4]) -> Result<MediaSourceKind> {
    let mut kind = None;
    for (name, provider) in names.into_iter().zip(MediaSourceKind::ALL) {
        match column::<i64>(row, name)? {
            0 => {}
            1 if kind.is_none() => kind = Some(provider),
            _ => return Err(invalid("ambiguous source kind")),
        }
    }
    kind.ok_or_else(|| invalid("missing source kind"))
}

pub(super) async fn validate_claim(
    tx: &mut Transaction<'_, Db>,
    claim: &ManagedLibraryOperationClaim,
    context: &Context,
) -> Result<()> {
    if context.library_id != claim.operation.library_id {
        return Err(invalid("claim library association"));
    }
    // The actual lease owns the library; a caller-supplied DTO is insufficient.
    // A publication child scan uses the existing parent lease identity.
    let library: Option<i64> = sqlx::query_scalar(
        r#"SELECT "LibraryId" FROM "ManagedLibraryLease" WHERE "OperationId"=? AND "OwnerToken"=? AND "LeaseOwner"=?"#,
    ).bind(claim.operation.parent_operation_id.unwrap_or(claim.operation.id))
        .bind(&claim.operation.owner_token).bind(&claim.lease_owner)
        .fetch_optional(&mut **tx).await.map_err(|_| storage())?;
    if library != Some(context.library_id) {
        return Err(invalid("lease library association"));
    }
    Ok(())
}
