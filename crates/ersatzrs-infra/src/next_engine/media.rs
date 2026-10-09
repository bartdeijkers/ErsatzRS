//! Host-authenticated media opens with current provider credentials.

use ersatzrs_core::repositories::{RemoteMediaSourceKind, RepositoryError, Result};
use ersatzrs_domain::PlayoutItem;
use reqwest::{Client, Response};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::{
    fmt::Write as _,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::OnceLock,
    time::Duration,
};
use subtle::ConstantTimeEq as _;
use tokio::time::{timeout_at, Instant};
use uuid::Uuid;

use super::{connections::PlexConnections, protocol::Source, schedule::ScheduledMedia, source};
use crate::{db::DbPool, repositories::SqlxResultExt as _};

pub const ACCESS_HEADER: &str = "x-ersatzrs-next-media";
/// Identifies one admitted Next process, including concurrent browser previews.
pub const SESSION_HEADER: &str = "x-ersatzrs-next-session";
/// Integrity of the occurrence carried by a private canvas request.
pub const CANVAS_HEADER: &str = "x-ersatzrs-next-canvas";
/// Integrity and purpose of one generated song background occurrence.
pub const SONG_HEADER: &str = "x-ersatzrs-next-song";
pub(super) const SESSION_ENV: &str = "ERSATZRS_NEXT_SESSION";
const OPEN_TIMEOUT: Duration = Duration::from_secs(15);

#[path = "external_media.rs"]
mod external;

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
        Uuid::parse_str(value).is_ok_and(|candidate| {
            candidate
                .as_bytes()
                .ct_eq(self.capability.as_bytes())
                .into()
        })
    }

    pub(super) fn source(&self, media: &ScheduledMedia) -> Result<Source> {
        let address = self
            .address
            .get()
            .ok_or_else(|| unavailable("media listener not configured"))?;
        Ok(Source::Http {
            probe_hint: None,
            is_live: None,
            uri: format!(
                "http://{address}/ffmpeg/next-media/{}/{}/{}",
                media.item.media_item_id, media.media_version_id, media.media_file_id
            ),
            headers: vec![format!("{ACCESS_HEADER}: {}", self.capability)],
            user_agent: None,
            keep_alive: Some(false),
            reconnect: Some(true),
            reconnect_delay_max: Some(2),
            timeout_us: Some(20_000_000),
            in_point_ms: None,
            out_point_ms: None,
        })
    }

    /// Publish one expiring graphics occurrence. Next supplies its process
    /// session header at playback time; the callback must validate that exact
    /// active session and re-read the current occurrence before rendering.
    /// # Errors
    /// Refuses an unconfigured listener or invalid occurrence interval.
    pub fn canvas_source(
        &self,
        binding: &super::canvas::CanvasBinding,
        probe_hint: super::protocol::ProbeHint,
    ) -> Result<Source> {
        if binding.finish <= binding.start
            || binding.expires <= binding.start
            || binding.in_point_ms < 0
            || binding.out_point_ms < 0
        {
            return Err(unavailable("invalid canvas binding"));
        }
        let address = self
            .address
            .get()
            .ok_or_else(|| unavailable("media listener not configured"))?;
        let encoded =
            serde_json::to_string(binding).map_err(|_| unavailable("invalid canvas binding"))?;
        let mut uri = url::Url::parse(&format!("http://{address}/ffmpeg/next-canvas"))
            .map_err(|_| unavailable("invalid canvas listener"))?;
        uri.query_pairs_mut().append_pair("binding", &encoded);
        Ok(Source::Http {
            uri: uri.to_string(),
            probe_hint: Some(probe_hint),
            is_live: Some(false),
            headers: vec![
                format!("{ACCESS_HEADER}: {}", self.capability),
                format!(
                    "{SESSION_HEADER}: {}",
                    binding.session.map_or_else(
                        || format!("{{{{{SESSION_ENV}}}}}"),
                        |session| session.to_string()
                    )
                ),
                format!("{CANVAS_HEADER}: {}", self.canvas_ticket(binding)?),
            ],
            user_agent: None,
            keep_alive: Some(true),
            reconnect: Some(false),
            reconnect_delay_max: None,
            timeout_us: Some(20_000_000),
            in_point_ms: None,
            out_point_ms: None,
        })
    }

    /// Publish one finite song image for an admitted process and exact occurrence.
    /// # Errors
    /// Refuses an unconfigured listener, invalid dimensions or interval.
    pub fn song_source(
        &self,
        binding: &super::song_background::SongBinding,
        probe_hint: super::protocol::ProbeHint,
    ) -> Result<Source> {
        let occurrence = &binding.occurrence;
        if binding.width <= 0
            || binding.height <= 0
            || occurrence.finish <= occurrence.start
            || occurrence.expires <= occurrence.start
            || occurrence.in_point_ms < 0
            || occurrence.out_point_ms < 0
        {
            return Err(unavailable("invalid song binding"));
        }
        let address = self
            .address
            .get()
            .ok_or_else(|| unavailable("media listener not configured"))?;
        let encoded =
            serde_json::to_string(binding).map_err(|_| unavailable("invalid song binding"))?;
        let mut uri = url::Url::parse(&format!("http://{address}/ffmpeg/next-song-background"))
            .map_err(|_| unavailable("invalid song listener"))?;
        uri.query_pairs_mut().append_pair("binding", &encoded);
        Ok(Source::Http {
            uri: uri.to_string(),
            probe_hint: Some(probe_hint),
            is_live: Some(false),
            headers: vec![
                format!("{ACCESS_HEADER}: {}", self.capability),
                format!(
                    "{SESSION_HEADER}: {}",
                    occurrence.session.map_or_else(
                        || format!("{{{{{SESSION_ENV}}}}}"),
                        |session| session.to_string()
                    )
                ),
                format!("{SONG_HEADER}: {}", self.song_ticket(binding)?),
            ],
            user_agent: None,
            keep_alive: Some(true),
            reconnect: Some(false),
            reconnect_delay_max: None,
            timeout_us: Some(20_000_000),
            in_point_ms: None,
            out_point_ms: None,
        })
    }

    /// Authenticate song purpose and expiry before opening a session or database.
    #[must_use]
    pub fn authorized_song(
        &self,
        binding: &super::song_background::SongBinding,
        ticket: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> bool {
        binding.occurrence.expires > now
            && self
                .song_ticket(binding)
                .is_ok_and(|expected| expected.as_bytes().ct_eq(ticket.as_bytes()).into())
    }

    fn song_ticket(&self, binding: &super::song_background::SongBinding) -> Result<String> {
        let encoded =
            serde_json::to_vec(binding).map_err(|_| unavailable("invalid song binding"))?;
        let mut digest = Sha256::new();
        digest.update(b"ersatzrs-next-song-v1");
        digest.update(self.capability.as_bytes());
        digest.update(encoded);
        let mut ticket = String::with_capacity(64);
        for byte in digest.finalize() {
            write!(ticket, "{byte:02x}").expect("writing to String cannot fail");
        }
        Ok(ticket)
    }

    /// Authenticate expiry and occurrence integrity before any database work.
    /// This does not replace active-session or current-item validation.
    #[must_use]
    pub fn authorized_canvas(
        &self,
        binding: &super::canvas::CanvasBinding,
        ticket: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> bool {
        binding.expires > now
            && self
                .canvas_ticket(binding)
                .is_ok_and(|expected| expected.as_bytes().ct_eq(ticket.as_bytes()).into())
    }

    fn canvas_ticket(&self, binding: &super::canvas::CanvasBinding) -> Result<String> {
        let encoded =
            serde_json::to_vec(binding).map_err(|_| unavailable("invalid canvas binding"))?;
        let mut digest = Sha256::new();
        digest.update(b"ersatzrs-next-canvas-v1");
        digest.update(self.capability.as_bytes());
        digest.update(encoded);
        let mut ticket = String::with_capacity(64);
        for byte in digest.finalize() {
            write!(ticket, "{byte:02x}").expect("writing to String cannot fail");
        }
        Ok(ticket)
    }

    pub(super) fn dynamic_source(
        &self,
        channel: i64,
        item: i64,
        digest: &str,
        audio: Option<u32>,
    ) -> Result<Source> {
        let address = self
            .address
            .get()
            .ok_or_else(|| unavailable("media listener not configured"))?;
        let audio = audio.map_or_else(|| "off".to_owned(), |index| index.to_string());
        Ok(Source::Dynamic {
            uri: format!("http://{address}/ffmpeg/next-item/{channel}/{item}/{digest}/{audio}"),
            headers: Some(vec![
                format!("{ACCESS_HEADER}: {}", self.capability),
                // Official Next expands headers from its own process environment.
                format!("{SESSION_HEADER}: {{{{{SESSION_ENV}}}}}"),
            ]),
            user_agent: None,
            timeout_us: Some(35_000_000),
        })
    }

    /// Resolve one published gap through the exact Next process session.
    pub(super) fn fallback_source(&self, channel: i64, start: i64, finish: i64) -> Result<Source> {
        let address = self
            .address
            .get()
            .ok_or_else(|| unavailable("media listener not configured"))?;
        Ok(Source::Dynamic {
            uri: format!("http://{address}/ffmpeg/next-fallback/{channel}/{start}/{finish}"),
            headers: Some(vec![
                format!("{ACCESS_HEADER}: {}", self.capability),
                format!("{SESSION_HEADER}: {{{{{SESSION_ENV}}}}}"),
            ]),
            user_agent: None,
            timeout_us: Some(35_000_000),
        })
    }

    /// A generated ASS track scoped to this host process and persisted item.
    pub(super) fn credits_source(
        &self,
        channel: i64,
        item: i64,
        media: i64,
        in_point_ms: u64,
        out_point_ms: u64,
    ) -> Result<Source> {
        let address = self
            .address
            .get()
            .ok_or_else(|| unavailable("media listener not configured"))?;
        Ok(Source::Http {
            uri: format!(
                "http://{address}/ffmpeg/next-credits/{channel}/{item}/{media}/{in_point_ms}/{out_point_ms}?ticket={}",
                self.credits_ticket(channel, item, media, in_point_ms, out_point_ms)
            ),
            probe_hint: None,
            is_live: Some(false),
            // libass opens this URL independently of Next and does not forward
            // the source headers. The URL ticket grants credits for one binding.
            headers: Vec::new(),
            user_agent: None,
            keep_alive: Some(false),
            reconnect: Some(false),
            reconnect_delay_max: None,
            timeout_us: Some(20_000_000),
            in_point_ms: Some(in_point_ms),
            out_point_ms: (out_point_ms != 0).then_some(out_point_ms),
        })
    }

    #[must_use]
    pub fn authorized_credits(&self, binding: (i64, i64, i64, u64, u64), ticket: &str) -> bool {
        let (channel, item, media, in_point_ms, out_point_ms) = binding;
        self.credits_ticket(channel, item, media, in_point_ms, out_point_ms)
            .as_bytes()
            .ct_eq(ticket.as_bytes())
            .into()
    }

    fn credits_ticket(
        &self,
        channel: i64,
        item: i64,
        media: i64,
        in_point_ms: u64,
        out_point_ms: u64,
    ) -> String {
        let mut digest = Sha256::new();
        digest.update(b"ersatzrs-next-credits-v1");
        digest.update(self.capability.as_bytes());
        for value in [channel, item, media] {
            digest.update(value.to_le_bytes());
        }
        digest.update(in_point_ms.to_le_bytes());
        digest.update(out_point_ms.to_le_bytes());
        let mut ticket = String::with_capacity(64);
        for byte in digest.finalize() {
            write!(ticket, "{byte:02x}").expect("writing to String cannot fail");
        }
        ticket
    }

    /// Address one prepared producer without exposing its command or provider URL.
    /// The caller must register it in the matching active session first. Fresh
    /// probe hints avoid a consuming probe before the single playback open.
    ///
    /// # Errors
    /// Returns a redacted error until the actual host listener is configured.
    pub fn prepared_source(
        &self,
        instance: Uuid,
        transfer: Uuid,
        probe_hint: super::protocol::ProbeHint,
        is_live: bool,
    ) -> Result<Source> {
        let address = self
            .address
            .get()
            .ok_or_else(|| unavailable("media listener not configured"))?;
        Ok(Source::Http {
            uri: format!("http://{address}/ffmpeg/next-prepared/{instance}/{transfer}"),
            probe_hint: Some(probe_hint),
            is_live: Some(is_live),
            headers: vec![format!("{ACCESS_HEADER}: {}", self.capability)],
            user_agent: None,
            // Next maps this to FFmpeg's multiple_requests=1. Its chunked
            // reader then retains EOF state instead of closing the connection
            // and reporting EIO on the next read. This does not enable retries.
            keep_alive: Some(true),
            reconnect: Some(false),
            reconnect_delay_max: None,
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
            candidate_overlays: None,
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
        open_http(self.http_client()?, &uri, &headers, range).await
    }

    fn http_client(&self) -> Result<&Client> {
        self.client
            .get_or_init(|| {
                Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .connect_timeout(Duration::from_secs(5))
                    .read_timeout(Duration::from_secs(10))
                    .build()
                    .ok()
            })
            .as_ref()
            .ok_or_else(|| unavailable("media HTTP client unavailable"))
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
