//! Immutable, owned artwork handles shared by native tasks and legacy uploads.
use super::*;

pub(super) async fn metadata(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    item_id: Option<EntityId>,
) -> Result<AddonMediaListNfoForm> {
    let row = if let Some(item_id) = item_id {
        sqlx::query(r#"SELECT "ProviderMetadataJson", "MetadataOverrideJson", "MetadataConflictJson" FROM "AddonMediaListItem" WHERE "Id"=? AND "AddonMediaListId"=?"#)
            .bind(item_id).bind(list_id).fetch_optional(&mut **tx).await.repository()?
    } else {
        sqlx::query(r#"SELECT "ProviderMetadataJson", "MetadataOverrideJson", "MetadataConflictJson" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id).fetch_optional(&mut **tx).await.repository()?
    }.ok_or(RepositoryError::NotFound { kind: "add-on artwork owner", id: item_id.unwrap_or(list_id) })?;
    metadata_form_from_json(
        &row.try_get::<String, _>("ProviderMetadataJson")
            .repository()?,
        &row.try_get::<String, _>("MetadataOverrideJson")
            .repository()?,
        row.try_get::<Option<String>, _>("MetadataConflictJson")
            .repository()?
            .as_deref(),
    )
}

pub(crate) async fn stage(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    item_id: Option<EntityId>,
    role: AddonMediaListArtworkRole,
    content_type: &str,
    bytes: &[u8],
) -> Result<String> {
    if role == AddonMediaListArtworkRole::Other {
        return Err(RepositoryError::Conflict(
            "uploaded artwork requires poster, fanart or thumbnail role".into(),
        ));
    }
    let (content_type, extension) = crate::addon_artwork::validate_image(content_type, bytes)
        .map_err(RepositoryError::Conflict)?;
    metadata(tx, list_id, item_id).await?;
    let scope = item_id.unwrap_or(0);
    let role = artwork_role_value(role);
    let hash = hex_hash(bytes);
    let now = encode_datetime(Utc::now());
    sqlx::query(
        r#"INSERT INTO "AddonMediaListArtworkUpload" (
        "AddonMediaListId", "AddonMediaListItemId", "ScopeItemId", "Role", "ContentType",
        "Extension", "Bytes", "Sha256", "CreatedAt", "UpdatedAt") VALUES (?,?,?,?,?,?,?,?,?,?)
        ON CONFLICT("AddonMediaListId", "ScopeItemId", "Role", "Sha256") DO NOTHING"#,
    )
    .bind(list_id)
    .bind(item_id)
    .bind(scope)
    .bind(role)
    .bind(content_type)
    .bind(extension)
    .bind(bytes)
    .bind(&hash)
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .repository()?;
    let id: EntityId = sqlx::query_scalar(
        r#"SELECT "Id" FROM "AddonMediaListArtworkUpload"
        WHERE "AddonMediaListId"=? AND "ScopeItemId"=? AND "Role"=? AND "Sha256"=?"#,
    )
    .bind(list_id)
    .bind(scope)
    .bind(role)
    .bind(hash)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    Ok(format!("{}{id}", crate::addon_artwork::UPLOAD_URL_PREFIX))
}

pub(super) async fn validate(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
    item_id: Option<EntityId>,
    metadata: &AddonMediaListNfoMetadata,
) -> Result<()> {
    if let Some(message) = invalid_artwork_message("metadata", metadata) {
        return Err(RepositoryError::Conflict(message));
    }
    for (url, role) in metadata
        .artwork
        .iter()
        .map(|a| (a.url.as_str(), Some(a.role)))
        .chain(metadata.artwork_urls.iter().map(|url| (url.as_str(), None)))
    {
        if let Some(id) = crate::addon_artwork::upload_id(url) {
            let owned: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtworkUpload"
                WHERE "Id"=? AND "AddonMediaListId"=? AND "ScopeItemId"=? AND (? IS NULL OR "Role"=?)"#)
                .bind(id).bind(list_id).bind(item_id.unwrap_or(0))
                .bind(role.map(artwork_role_value)).bind(role.map(artwork_role_value))
                .fetch_one(&mut **tx).await.repository()?;
            if owned != 1 {
                return Err(RepositoryError::Conflict(
                    "artwork handle does not belong to this list, item and role".into(),
                ));
            }
        }
    }
    Ok(())
}
