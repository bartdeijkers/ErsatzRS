//! ErsatzTV #2990: replace a Plex device identity only after successful sign-in.

use super::*;
use serde::{Deserialize, Serialize};

const PENDING_FLOW_KEY: &str = "plex.pending_flow";
const PIN_LIFETIME_SECONDS: i64 = 120;
const ACCOUNT_CREDENTIALS_SQL: &str = r#"SELECT
    (SELECT "Value" FROM "ConfigElement" WHERE "Key" = 'plex.client_identifier'),
    (SELECT "Value" FROM "ConfigElement" WHERE "Key" = 'plex.user_auth_token')"#;

#[derive(Serialize, Deserialize)]
struct PendingFlow {
    id: String,
    code: String,
    client_identifier: String,
    expires_at: i64,
}

impl PlexService {
    /// Refresh a complete Plex account inventory without deleting missing servers.
    ///
    /// Missing credentials skip discovery; HTTP 401 clears only the rejected
    /// credential generation, retaining media and any pending reauthentication.
    ///
    /// # Errors
    /// Returns a redacted provider/storage error or rejects a superseded inventory.
    pub async fn refresh_account_servers(&self, pool: &DbPool) -> Result<bool> {
        let identity: (Option<String>, Option<String>) = sqlx::query_as(ACCOUNT_CREDENTIALS_SQL)
            .fetch_one(pool)
            .await
            .repository()?;
        let (Some(client), Some(token)) = &identity else {
            return Ok(false);
        };
        if client.trim().is_empty() || token.trim().is_empty() {
            return Ok(false);
        }
        let servers = match self.get_servers(token, client).await {
            Ok(servers) => servers,
            Err(error) => {
                if matches!(&error, RepositoryError::Unavailable { operation, .. } if *operation == "Plex authorization")
                {
                    clear_rejected_credentials(pool, &identity).await?;
                }
                return Err(error);
            }
        };
        let mut tx = crate::db::begin_write(pool).await.repository()?;
        let current: (Option<String>, Option<String>) = sqlx::query_as(ACCOUNT_CREDENTIALS_SQL)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
        if current != identity {
            return Err(authentication_error(
                "account changed during server discovery",
            ));
        }
        reconcile_plex_sources(&mut tx, &servers).await?;
        tx.commit().await.repository()?;
        Ok(true)
    }

    /// Start a bounded PIN flow, preserving existing credentials during reauthentication.
    ///
    /// # Errors
    /// Returns a redacted provider or persistence error; existing credentials remain valid.
    pub async fn start_account_authentication(
        &self,
        pool: &DbPool,
        force_new_credentials: bool,
    ) -> Result<PlexAuthPin> {
        let client_identifier = if force_new_credentials {
            uuid::Uuid::new_v4().to_string()
        } else {
            Self::get_or_create_client_identifier(pool).await?
        };
        let pin = self.start_pin_flow(&client_identifier).await?;
        let flow = PendingFlow {
            id: pin.id.clone(),
            code: pin.code.clone(),
            client_identifier,
            expires_at: Utc::now().timestamp() + PIN_LIFETIME_SECONDS,
        };
        let encoded = serde_json::to_string(&flow)
            .map_err(|_| authentication_error("cannot encode pending sign-in"))?;
        sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(PENDING_FLOW_KEY)
            .bind(encoded)
            .execute(pool)
            .await
            .repository()?;
        Ok(pin)
    }

    /// Poll the current PIN once. Only completion status leaves this boundary.
    ///
    /// A stale, failed or expired flow cannot overwrite working credentials.
    ///
    /// # Errors
    /// Returns a redacted error for expired/superseded flows, provider failure or storage failure.
    pub async fn complete_account_authentication(
        &self,
        pool: &DbPool,
        pin_id: &str,
    ) -> Result<bool> {
        let encoded: String =
            sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(PENDING_FLOW_KEY)
                .fetch_optional(pool)
                .await
                .repository()?
                .ok_or_else(|| authentication_error("sign-in is no longer pending"))?;
        let flow: PendingFlow = serde_json::from_str(&encoded)
            .map_err(|_| authentication_error("pending sign-in is invalid"))?;
        if flow.id != pin_id {
            return Err(authentication_error("sign-in was superseded"));
        }
        let remaining = flow.expires_at - Utc::now().timestamp();
        if remaining <= 0 {
            clear_pending(pool, &encoded).await?;
            return Err(authentication_error("sign-in expired; start again"));
        }
        let remaining = u64::try_from(remaining)
            .map_err(|_| authentication_error("sign-in deadline is invalid"))?;
        let result = tokio::time::timeout(std::time::Duration::from_secs(remaining), async {
            let Some(token) = self
                .complete_pin_flow(&flow.id, &flow.code, &flow.client_identifier)
                .await?
            else {
                return Ok(None);
            };
            self.validate_account_token(&flow.client_identifier, &token)
                .await
                .map(Some)
        })
        .await
        .unwrap_or_else(|_| Err(authentication_error("sign-in expired; start again")));
        match result {
            Ok(None) => Ok(false),
            Ok(Some(token)) => {
                commit_credentials(pool, &encoded, &flow, &token).await?;
                Ok(true)
            }
            Err(error) => {
                clear_pending(pool, &encoded).await?;
                Err(error)
            }
        }
    }

    async fn validate_account_token(&self, client_identifier: &str, token: &str) -> Result<String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UserResponse {
            auth_token: String,
        }
        let user: UserResponse = self
            .client
            .get(format!("{}/api/v2/user", self.plex_tv_base))
            .timeout(std::time::Duration::from_secs(10))
            .header(ACCEPT, APPLICATION_JSON)
            .header("X-Plex-Product", PLEX_APP_NAME)
            .header("X-Plex-Version", env!("CARGO_PKG_VERSION"))
            .header("X-Plex-Client-Identifier", client_identifier)
            .header("X-Plex-Token", token)
            .send()
            .await
            .map_err(provider_error)?
            .error_for_status()
            .map_err(provider_error)?
            .json()
            .await
            .map_err(provider_error)?;
        if user.auth_token.trim().is_empty() {
            return Err(authentication_error(
                "authorized account did not return credentials",
            ));
        }
        Ok(user.auth_token)
    }
}

async fn clear_pending(pool: &DbPool, encoded: &str) -> Result<()> {
    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ? AND "Value" = ?"#)
        .bind(PENDING_FLOW_KEY)
        .bind(encoded)
        .execute(pool)
        .await
        .repository()?;
    Ok(())
}

async fn clear_rejected_credentials(
    pool: &DbPool,
    identity: &(Option<String>, Option<String>),
) -> Result<()> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let current: (Option<String>, Option<String>) = sqlx::query_as(ACCOUNT_CREDENTIALS_SQL)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
    if &current == identity {
        sqlx::query(
            r#"DELETE FROM "ConfigElement" WHERE "Key" IN (
            'plex.client_identifier', 'plex.user_auth_token', 'plex.current_server_auth_token')
            OR "Key" GLOB 'plex.server_auth_token.*'"#,
        )
        .execute(&mut *tx)
        .await
        .repository()?;
    }
    tx.commit().await.repository()
}

async fn commit_credentials(
    pool: &DbPool,
    encoded: &str,
    flow: &PendingFlow,
    token: &str,
) -> Result<()> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    if flow.expires_at <= Utc::now().timestamp() {
        return Err(authentication_error("sign-in expired; start again"));
    }
    let claimed = sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ? AND "Value" = ?"#)
        .bind(PENDING_FLOW_KEY)
        .bind(encoded)
        .execute(&mut *tx)
        .await
        .repository()?;
    if claimed.rows_affected() != 1 {
        return Err(authentication_error("sign-in was superseded"));
    }
    for (key, value) in [
        (PLEX_CLIENT_IDENTIFIER_KEY, flow.client_identifier.as_str()),
        (PLEX_USER_AUTH_TOKEN_KEY, token),
    ] {
        sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    tx.commit().await.repository()
}

fn authentication_error(message: &str) -> RepositoryError {
    RepositoryError::unavailable("Plex sign-in", message)
}

#[cfg(test)]
#[path = "authentication_tests.rs"]
mod tests;
