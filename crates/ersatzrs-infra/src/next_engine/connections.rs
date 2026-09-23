//! Late Plex connection selection for the Next media-open adapter.
//!
//! Maps `GetPlexConnectionParametersHandler` and `PlexBaseConnectionHandler`:
//! retain a working active connection, otherwise probe alternatives and activate
//! the first verified server. Caller cancellation drops probes; no detached work
//! survives the request. Never invoke this while holding a playout build lock.

use std::{collections::HashMap, sync::Arc, time::Duration};

use ersatzrs_core::repositories::{MediaSourceRepository, RemoteMediaSourceKind};
use ersatzrs_domain::{PlexConnection, PlexMediaSource};
use futures_util::{stream, StreamExt as _};
use reqwest::{redirect::Policy, Client};
use serde::Deserialize;
use tokio::{
    sync::Mutex,
    time::{timeout_at, Instant},
};

use super::source::{scoped_credential, server_url};
use crate::{db::DbPool, repositories::SqliteMediaSourceRepository};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const CACHE_LIFETIME: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const PROBE_CONCURRENCY: usize = 4;

/// Redacted outcomes for connection discovery and persistence.
#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("Next media connection configuration is unavailable")]
    Configuration,
    #[error("Next media connection discovery timed out")]
    Timeout,
    #[error("No verified Plex connection is reachable")]
    Unreachable,
    #[error("Plex connection changed during discovery")]
    Changed,
    #[error("Plex connection selection could not be persisted")]
    Persistence,
}

/// Connection values are private transport data; deliberately no Debug.
#[derive(Clone)]
pub struct PlexTransport {
    pub address: String,
    pub token: String,
}

struct CachedConnection {
    identity: String,
    connections: Vec<PlexConnection>,
    transport: PlexTransport,
    expires: Instant,
}

type SourceGate = Arc<Mutex<Option<CachedConnection>>>;

/// Shared by the host's media-open requests. Each source has one discovery gate,
/// so concurrent video/audio/subtitle opens reuse a verified connection.
pub struct PlexConnections {
    client: Client,
    sources: Mutex<HashMap<i64, SourceGate>>,
}

impl PlexConnections {
    /// Construct a bounded identity-probe client. Redirects are refused so an
    /// unverified endpoint cannot forward provider credentials to another host.
    /// # Errors
    /// Returns a redacted client-initialization error.
    pub fn new() -> Result<Self, ConnectionError> {
        let client = Client::builder()
            .redirect(Policy::none())
            .timeout(PROBE_TIMEOUT)
            .build()
            .map_err(|_| ConnectionError::Configuration)?;
        Ok(Self {
            client,
            sources: Mutex::default(),
        })
    }

    /// Resolve current credentials and verify the configured Plex machine.
    ///
    /// The supplied deadline bounds gate acquisition, database reads, all probes
    /// and persistence together. A successful cache entry is invalidated by any
    /// connection, server identity or credential change before it can be reused.
    /// Failed discovery never clears the previous active selection.
    /// # Errors
    /// Returns only redacted timeout, configuration, discovery or storage errors.
    pub async fn resolve(
        &self,
        pool: &DbPool,
        source_id: i64,
        deadline: Instant,
    ) -> Result<PlexTransport, ConnectionError> {
        timeout_at(deadline, self.resolve_inner(pool, source_id))
            .await
            .map_err(|_| ConnectionError::Timeout)?
    }

    async fn resolve_inner(
        &self,
        pool: &DbPool,
        source_id: i64,
    ) -> Result<PlexTransport, ConnectionError> {
        let gate = self
            .sources
            .lock()
            .await
            .entry(source_id)
            .or_default()
            .clone();
        let mut cached = gate.lock().await;
        let mut source = SqliteMediaSourceRepository(pool.clone())
            .get_plex(source_id)
            .await
            .map_err(|_| ConnectionError::Configuration)?;
        let identity = source
            .machine_identifier
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or(ConnectionError::Configuration)?
            .to_owned();
        let legacy = crate::plex::load_plex_token(pool, &identity)
            .await
            .map_err(|_| ConnectionError::Configuration)?;
        let token = scoped_credential(
            pool,
            RemoteMediaSourceKind::Plex,
            &[format!("plex.server_auth_token.{identity}")],
            legacy,
        )
        .await
        .map_err(|_| ConnectionError::Configuration)?;
        if token.chars().any(char::is_control) {
            return Err(ConnectionError::Configuration);
        }
        if let Some(entry) = cached.as_ref().filter(|entry| {
            entry.expires > Instant::now()
                && entry.identity == identity
                && entry.connections == source.connections
                && entry.transport.token == token
        }) {
            return Ok(entry.transport.clone());
        }
        let selected = self.choose(&source, &identity, &token).await?;
        activate(pool, &source, &selected).await?;
        for connection in &mut source.connections {
            connection.is_active = connection.id == selected.id;
        }
        let transport = PlexTransport {
            address: selected.uri,
            token,
        };
        *cached = Some(CachedConnection {
            identity,
            connections: source.connections,
            transport: transport.clone(),
            expires: Instant::now() + CACHE_LIFETIME,
        });
        Ok(transport)
    }

    async fn choose(
        &self,
        source: &PlexMediaSource,
        identity: &str,
        token: &str,
    ) -> Result<PlexConnection, ConnectionError> {
        let active = source.connections.iter().find(|c| c.is_active);
        if let Some(active) = active {
            if self.probe(active, identity, token).await {
                return Ok(active.clone());
            }
        }
        let candidates = source
            .connections
            .iter()
            .filter(|c| active.is_none_or(|a| a.id != c.id))
            .cloned()
            .collect::<Vec<_>>();
        let mut probes = stream::iter(candidates.into_iter().map(|connection| async move {
            self.probe(&connection, identity, token)
                .await
                .then_some(connection)
        }))
        .buffer_unordered(PROBE_CONCURRENCY);
        while let Some(result) = probes.next().await {
            if let Some(connection) = result {
                return Ok(connection);
            }
        }
        Err(ConnectionError::Unreachable)
    }

    async fn probe(&self, connection: &PlexConnection, identity: &str, token: &str) -> bool {
        let Ok(url) = server_url(&connection.uri) else {
            return false;
        };
        let Ok(mut response) = self
            .client
            .get(url)
            .header("X-Plex-Token", token)
            .header("Accept", "text/xml")
            .send()
            .await
        else {
            return false;
        };
        if !response.status().is_success() {
            return false;
        }
        let mut body = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) if body.len().saturating_add(chunk.len()) <= MAX_RESPONSE_BYTES => {
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                _ => return false,
            }
        }
        matches!(quick_xml::de::from_reader(body.as_slice()),
            Ok(IdentityDocument::MediaContainer { machine_identifier }) if machine_identifier == identity)
    }
}

#[derive(Deserialize)]
enum IdentityDocument {
    MediaContainer {
        #[serde(rename = "@machineIdentifier")]
        machine_identifier: String,
    },
}

async fn activate(
    pool: &DbPool,
    source: &PlexMediaSource,
    selected: &PlexConnection,
) -> Result<(), ConnectionError> {
    let mut tx = crate::db::begin_write(pool)
        .await
        .map_err(|_| ConnectionError::Persistence)?;
    // A connection edited/deleted during its HTTP probe must not be activated.
    let unchanged: bool = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1
        FROM "PlexConnection" c JOIN "PlexMediaSource" s ON s."Id" = c."PlexMediaSourceId"
        WHERE c."Id" = ? AND c."PlexMediaSourceId" = ? AND c."Uri" = ? AND s."ClientIdentifier" = ?)"#)
        .bind(selected.id).bind(source.media_source.id).bind(&selected.uri)
        .bind(&source.machine_identifier).fetch_one(&mut *tx).await
        .map_err(|_| ConnectionError::Persistence)?;
    if !unchanged {
        return Err(ConnectionError::Changed);
    }
    sqlx::query(
        r#"UPDATE "PlexConnection" SET "IsActive" = ("Id" = ?) WHERE "PlexMediaSourceId" = ?"#,
    )
    .bind(selected.id)
    .bind(source.media_source.id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ConnectionError::Persistence)?;
    tx.commit().await.map_err(|_| ConnectionError::Persistence)
}

#[cfg(test)]
#[path = "connections_tests.rs"]
mod tests;
