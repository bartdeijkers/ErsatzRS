//! The existing local scanner write fence, shared by metadata and teletext.

use ersatzrs_core::repositories::{ManagedLibraryOperationClaim, RepositoryError};
use ersatzrs_infra::db::Db;
use ersatzrs_infra::repositories::claim_is_current;
use sqlx::Transaction;

pub(crate) async fn check(
    tx: &mut Transaction<'_, Db>,
    library_path_id: i64,
    claim: Option<&ManagedLibraryOperationClaim>,
) -> crate::Result<()> {
    let Some(claim) = claim else { return Ok(()) };
    let library_matches: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "LibraryPath" p
           JOIN "ManagedLibraryLease" l ON l."LibraryId"=p."LibraryId"
           WHERE p."Id"=? AND p."LibraryId"=? AND l."OperationId"=?
             AND l."OwnerToken"=? AND l."LeaseOwner"=?)"#,
    )
    .bind(library_path_id)
    .bind(claim.operation.library_id)
    .bind(
        claim
            .operation
            .parent_operation_id
            .unwrap_or(claim.operation.id),
    )
    .bind(&claim.operation.owner_token)
    .bind(&claim.lease_owner)
    .fetch_one(&mut **tx)
    .await?;
    if !library_matches || !claim_is_current(tx, claim).await? {
        return Err(
            RepositoryError::Conflict("local scan claim is no longer current".into()).into(),
        );
    }
    Ok(())
}
