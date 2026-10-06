//! Expiring media-part tickets for native and Next external JSON playback.
//! Uses the host's existing private listener, capability and HTTP relay.

use super::*;
use crate::external_json::playback::part_key;
use chrono::{DateTime, Utc};

impl MediaRelay {
    /// Bind one transient Plex part to this host instance and an expiry time.
    /// The URL contains no provider address or credential and requires no media
    /// rows or long-lived registry entries.
    ///
    /// # Errors
    /// Rejects invalid parts, IDs, expired tickets or an unconfigured listener.
    pub fn external_source_url(
        &self,
        source_id: i64,
        part: &str,
        expires: DateTime<Utc>,
    ) -> Result<String> {
        let part = part_key(part).map_err(|_| unavailable("invalid external media part"))?;
        if source_id <= 0 || expires <= Utc::now() {
            return Err(unavailable("invalid external media lifetime"));
        }
        let address = self
            .address
            .get()
            .ok_or_else(|| unavailable("media listener not configured"))?;
        let expires = expires.timestamp();
        let ticket = self.external_ticket(source_id, expires, &part);
        let mut url = url::Url::parse(&format!(
            "http://{address}/ffmpeg/external-media/{source_id}/{expires}"
        ))
        .map_err(|_| unavailable("invalid media listener address"))?;
        url.query_pairs_mut()
            .append_pair("part", &part)
            .append_pair("ticket", &ticket);
        Ok(url.into())
    }

    /// Verify exact source/part/expiry binding before reading credentials.
    #[must_use]
    pub fn authorized_external(
        &self,
        source_id: i64,
        expires: i64,
        part: &str,
        ticket: &str,
    ) -> bool {
        source_id > 0
            && expires >= Utc::now().timestamp()
            && part_key(part).is_ok_and(|normalized| normalized == part)
            && bool::from(
                self.external_ticket(source_id, expires, part)
                    .as_bytes()
                    .ct_eq(ticket.as_bytes()),
            )
    }

    fn external_ticket(&self, source_id: i64, expires: i64, part: &str) -> String {
        let mut digest = Sha256::new();
        digest.update(b"ersatzrs-external-media-v1");
        digest.update(self.capability.as_bytes());
        digest.update(source_id.to_le_bytes());
        digest.update(expires.to_le_bytes());
        // Fixed-length input, like the existing credits ticket, even for a
        // variable-length media locator. Never expose the host capability.
        digest.update(Sha256::digest(part.as_bytes()));
        let mut ticket = String::with_capacity(64);
        for byte in digest.finalize() {
            write!(ticket, "{byte:02x}").expect("writing to String cannot fail");
        }
        ticket
    }

    /// Open a signed external media part with the source's current credentials.
    /// Dropping the returned streaming response cancels its upstream body.
    ///
    /// # Errors
    /// Returns redacted ticket, source, connection, credential and HTTP failures.
    pub async fn open_external(
        &self,
        pool: &DbPool,
        source_id: i64,
        expires: i64,
        part: &str,
        ticket: &str,
        range: Option<&str>,
    ) -> Result<Response> {
        if !self.authorized_external(source_id, expires, part, ticket) {
            return Err(unavailable("external media ticket is invalid or expired"));
        }
        timeout_at(Instant::now() + OPEN_TIMEOUT, async {
            let config = crate::plex::load_active_plex_connection_config(pool, source_id)
                .await
                .map_err(|_| unavailable("external Plex connection or credential unavailable"))?;
            let base = source::server_url(&config.uri)?;
            let uri = base
                .join(part)
                .map_err(|_| unavailable("invalid external media part"))?;
            if uri.origin() != base.origin() {
                return Err(unavailable("invalid external media origin"));
            }
            let headers = [format!("X-Plex-Token: {}", config.token)];
            open_http(self.http_client()?, uri.as_str(), &headers, range).await
        })
        .await
        .map_err(|_| unavailable("external media open timed out"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_media_tickets_bind_part_source_expiry_and_host_instance() {
        let relay = MediaRelay::new();
        relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
        let expires = Utc::now() + chrono::Duration::hours(2);
        let part = "/library/parts/12/file.mkv?download=1";
        let uri = relay.external_source_url(10, part, expires).unwrap();
        let url = url::Url::parse(&uri).unwrap();
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        let ticket = query.get("ticket").unwrap();
        assert_eq!(query.get("part").unwrap().as_ref(), part);
        assert!(relay.authorized_external(10, expires.timestamp(), part, ticket));
        assert!(!relay.authorized_external(11, expires.timestamp(), part, ticket));
        assert!(!relay.authorized_external(10, expires.timestamp() + 1, part, ticket));
        assert!(!relay.authorized_external(
            10,
            expires.timestamp(),
            "/library/parts/13/file.mkv",
            ticket
        ));
        assert!(!MediaRelay::new().authorized_external(10, expires.timestamp(), part, ticket));
        assert!(!relay.authorized_external(10, expires.timestamp(), part, ""));
        let elapsed_deadline = Utc::now().timestamp() - 1;
        let expired_ticket = relay.external_ticket(10, elapsed_deadline, part);
        assert!(!relay.authorized_external(10, elapsed_deadline, part, &expired_ticket));
        assert!(relay
            .external_source_url(10, "https://example.invalid/library/parts/12", expires)
            .is_err());
    }
}
