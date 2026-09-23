//! Host-authenticated media opens with current provider credentials.

use ersatzrs_core::repositories::{RemoteMediaSourceKind, RepositoryError, Result};
use ersatzrs_domain::PlayoutItem;
use reqwest::{Client, Response};
use sqlx::Row;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::OnceLock,
    time::Duration,
};
use tokio::time::{timeout_at, Instant};
use uuid::Uuid;

use super::{connections::PlexConnections, protocol::Source, schedule::ScheduledMedia, source};
use crate::{db::DbPool, repositories::SqlxResultExt as _};

pub const ACCESS_HEADER: &str = "x-ersatzrs-next-media";
const OPEN_TIMEOUT: Duration = Duration::from_secs(15);

/// One process-local capability. Its published URLs contain only binding IDs;
/// provider credentials are read when media is opened, never put in the URL.
/// Deliberately no Debug implementation.
pub struct MediaRelay {
    address: OnceLock<SocketAddr>,
    capability: Uuid,
    client: OnceLock<Option<Client>>,
    plex: OnceLock<Option<PlexConnections>>,
}

impl Default for MediaRelay {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaRelay {
    #[must_use]
    pub fn new() -> Self {
        Self {
            address: OnceLock::new(),
            capability: Uuid::new_v4(),
            client: OnceLock::new(),
            plex: OnceLock::new(),
        }
    }

    /// Set the actual bound host address, never a client-supplied Host header.
    /// # Errors
    /// Refuses reconfiguration to another listener within a process lifetime.
    pub fn configure(&self, mut address: SocketAddr) -> Result<()> {
        if address.ip().is_unspecified() {
            address.set_ip(if address.is_ipv4() {
                IpAddr::V4(Ipv4Addr::LOCALHOST)
            } else {
                IpAddr::V6(Ipv6Addr::LOCALHOST)
            });
        }
        if self.address.get().is_some_and(|old| *old == address) {
            return Ok(());
        }
        self.address
            .set(address)
            .map_err(|_| unavailable("media listener already configured"))
    }

    #[must_use]
    pub fn authorized(&self, value: &str) -> bool {
        Uuid::parse_str(value).is_ok_and(|candidate| candidate == self.capability)
    }

    pub(super) fn source(&self, media: &ScheduledMedia) -> Result<Source> {
        let address = self
            .address
            .get()
            .ok_or_else(|| unavailable("media listener not configured"))?;
        Ok(Source::Http {
            uri: format!(
                "http://{address}/ffmpeg/next-media/{}/{}/{}",
                media.item.media_item_id, media.media_version_id, media.media_file_id
            ),
            headers: vec![format!("{ACCESS_HEADER}: {}", self.capability)],
            user_agent: None,
            reconnect: Some(true),
            reconnect_delay_max: Some(2),
            timeout_us: Some(20_000_000),
            in_point_ms: None,
            out_point_ms: None,
        })
    }

    /// Open one exact persisted binding with current credentials. Call only after
    /// authenticating the internal capability. No schedule locks are acquired.
    /// The caller streams the response; dropping it cancels the upstream body.
    /// # Errors
    /// Returns redacted errors for invalid bindings, discovery or HTTP failures.
    pub async fn open(
        &self,
        pool: &DbPool,
        binding: (i64, i64, i64),
        range: Option<&str>,
    ) -> Result<Response> {
        let deadline = Instant::now() + OPEN_TIMEOUT;
        timeout_at(deadline, self.open_inner(pool, binding, range, deadline))
            .await
            .map_err(|_| unavailable("media open timed out"))?
    }

    async fn open_inner(
        &self,
        pool: &DbPool,
        binding: (i64, i64, i64),
        range: Option<&str>,
        deadline: Instant,
    ) -> Result<Response> {
        let (item_id, version_id, file_id) = binding;
        let row = sqlx::query(r#"SELECT f."Path", l."MediaSourceId",
            EXISTS(SELECT 1 FROM "PlexLibrary" WHERE "Id"=l."Id") AS "Plex",
            EXISTS(SELECT 1 FROM "JellyfinLibrary" WHERE "Id"=l."Id") AS "Jellyfin",
            EXISTS(SELECT 1 FROM "EmbyLibrary" WHERE "Id"=l."Id") AS "Emby"
            FROM "MediaFile" f JOIN "MediaVersion" v ON v."Id"=f."MediaVersionId"
            JOIN "MediaItem" m ON m."Id"=? JOIN "LibraryPath" p ON p."Id"=m."LibraryPathId"
            JOIN "Library" l ON l."Id"=p."LibraryId"
            WHERE v."Id"=? AND f."Id"=? AND
            (v."MovieId"=m."Id" OR v."EpisodeId"=m."Id" OR v."MusicVideoId"=m."Id" OR v."OtherVideoId"=m."Id")"#)
            .bind(item_id).bind(version_id).bind(file_id).fetch_optional(pool).await.repository()?
            .ok_or_else(|| unavailable("media binding is unavailable"))?;
        let mut kinds = Vec::new();
        for (column, kind) in [
            ("Plex", RemoteMediaSourceKind::Plex),
            ("Jellyfin", RemoteMediaSourceKind::Jellyfin),
            ("Emby", RemoteMediaSourceKind::Emby),
        ] {
            if row.try_get::<bool, _>(column).repository()? {
                kinds.push(kind);
            }
        }
        let [kind] = kinds.as_slice() else {
            return Err(unavailable("media provider is unavailable"));
        };
        let media = ScheduledMedia {
            item: PlayoutItem {
                media_item_id: item_id,
                ..Default::default()
            },
            media_version_id: version_id,
            media_file_id: file_id,
            path: row.try_get("Path").repository()?,
            remote_stream_id: None,
        };
        let plex = self
            .plex
            .get_or_init(|| PlexConnections::new().ok())
            .as_ref()
            .ok_or_else(|| unavailable("media connection client unavailable"))?;
        let resolved = source::resolve_http(
            pool,
            &media,
            row.try_get("MediaSourceId").repository()?,
            *kind,
            Some((plex, deadline)),
        )
        .await?;
        let Source::Http { uri, headers, .. } = resolved else {
            return Err(unavailable("HTTP media source is unavailable"));
        };
        let client = self
            .client
            .get_or_init(|| {
                Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .connect_timeout(Duration::from_secs(5))
                    .read_timeout(Duration::from_secs(10))
                    .build()
                    .ok()
            })
            .as_ref()
            .ok_or_else(|| unavailable("media HTTP client unavailable"))?;
        open_http(client, &uri, &headers, range).await
    }
}

/// Follow media redirects within the open deadline. Credentials are never
/// forwarded across origins, even if a later hop returns to the original host.
async fn open_http(
    client: &Client,
    uri: &str,
    headers: &[String],
    range: Option<&str>,
) -> Result<Response> {
    let mut url = url::Url::parse(uri).map_err(|_| unavailable("invalid media URL"))?;
    let mut forward_credentials = true;
    for hop in 0..=5 {
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(unavailable("invalid media redirect"));
        }
        let mut request = client.get(url.clone());
        if forward_credentials {
            for header in headers {
                let (name, value) = header
                    .split_once(':')
                    .ok_or_else(|| unavailable("invalid media header"))?;
                request = request.header(name, value.trim());
            }
        }
        if let Some(range) = range {
            request = request.header(reqwest::header::RANGE, range);
        }
        let response = request
            .send()
            .await
            .map_err(|_| unavailable("media request failed"))?;
        if response.status().is_success()
            || response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE
        {
            return Ok(response);
        }
        if hop == 5 || !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            return Err(unavailable("media server refused request"));
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| unavailable("invalid media redirect"))?;
        let next = url
            .join(location)
            .map_err(|_| unavailable("invalid media redirect"))?;
        forward_credentials &= next.origin() == url.origin();
        url = next;
    }
    Err(unavailable("media redirect limit reached"))
}

fn unavailable(message: &'static str) -> RepositoryError {
    RepositoryError::unavailable("Next media open", message)
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
