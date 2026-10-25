use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{
    decode, decode_header, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::host::state::{AppState, IptvAccessTokenVerifier};

/// Holder for a propagated `?access_token=` query parameter.
///
/// Ports the `accessTokenUri`/`accessTokenUriAmp` pattern from
/// `ErsatzTV.Core/Iptv/ChannelPlaylist.cs` and `ErsatzTV/Controllers/IptvController.cs`:
/// the first URL on a page uses `?access_token=…`; subsequent URIs that
/// already carry a query string use `&access_token=…`.
#[derive(Debug, Clone, Default)]
pub struct AccessToken {
    token: Option<String>,
}

impl AccessToken {
    #[must_use]
    pub fn new(token: Option<impl Into<String>>) -> Self {
        Self {
            token: token.and_then(|value| {
                let value = value.into();
                if value.trim().is_empty() {
                    None
                } else {
                    Some(value)
                }
            }),
        }
    }

    #[must_use]
    pub fn query(&self) -> String {
        match &self.token {
            Some(token) => format!("?access_token={token}"),
            None => String::new(),
        }
    }

    #[must_use]
    pub fn query_amp(&self) -> String {
        match &self.token {
            Some(token) => format!("&access_token={token}"),
            None => String::new(),
        }
    }
}

pub async fn require_iptv_access(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if !state.iptv_access_token_verifier.is_enabled() {
        return next.run(request).await;
    }

    if request.uri().path().starts_with("/iptv/logos") {
        return next.run(request).await;
    }

    let token = extract_bearer_token(request.headers().get(header::AUTHORIZATION))
        .or_else(|| extract_access_token(request.uri()));

    match token {
        Some(token)
            if state
                .iptv_access_token_verifier
                .verify_for_path(&token, request.uri().path()) =>
        {
            next.run(request).await
        }
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

fn extract_access_token(uri: &Uri) -> Option<String> {
    let query = uri.query()?;
    query.split('&').find_map(|segment| {
        let (key, value) = segment.split_once('=')?;
        if key != "access_token" {
            return None;
        }

        let decoded = urlencoding::decode(value).ok()?;
        let token = decoded.trim();
        if token.is_empty() {
            None
        } else {
            Some(token.to_string())
        }
    })
}

fn extract_bearer_token(header_value: Option<&HeaderValue>) -> Option<String> {
    let value = header_value?.to_str().ok()?.trim();
    let token = value.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

impl IptvAccessTokenVerifier {
    #[must_use]
    pub fn from_optional_secret(secret: Option<String>) -> Self {
        let Some(secret) = secret.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }) else {
            return Self::disabled();
        };

        Self::enabled(&secret)
    }

    #[must_use]
    pub fn disabled() -> Self {
        Self {
            validation: None,
            decoding_key: None,
            encoding_key: None,
        }
    }

    #[must_use]
    pub fn enabled(secret: &str) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_aud = false;
        validation.set_required_spec_claims(&["exp"]);

        Self {
            validation: Some(validation),
            decoding_key: Some(DecodingKey::from_secret(secret.as_bytes())),
            encoding_key: Some(EncodingKey::from_secret(secret.as_bytes())),
        }
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.validation.is_some() && self.decoding_key.is_some()
    }

    #[must_use]
    pub fn verify(&self, token: &str) -> bool {
        self.verified_claims(token).is_some()
    }

    #[must_use]
    pub fn verify_for_path(&self, token: &str, path: &str) -> bool {
        let Some(claims) = self.verified_claims(token) else {
            return false;
        };
        let Some(scope) = claims.get("scope").and_then(Value::as_str) else {
            return true;
        };
        if scope != "channel-preview" {
            return false;
        }
        let Some(channel) = claims.get("channel").and_then(Value::as_str) else {
            return false;
        };
        preview_path_channel(path).is_some_and(|path_channel| path_channel == channel)
    }

    /// Issue a short-lived JWT scoped to one channel preview.
    pub fn issue_preview_token(
        &self,
        channel: &str,
    ) -> Result<Option<String>, jsonwebtoken::errors::Error> {
        let Some(encoding_key) = &self.encoding_key else {
            return Ok(None);
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let claims = PreviewClaims {
            exp: now.saturating_add(4 * 60 * 60),
            scope: "channel-preview",
            channel,
        };
        encode(&Header::new(Algorithm::HS256), &claims, encoding_key).map(Some)
    }

    fn verified_claims(&self, token: &str) -> Option<Value> {
        let (Some(validation), Some(decoding_key)) = (&self.validation, &self.decoding_key) else {
            return Some(Value::Object(serde_json::Map::default()));
        };

        let Ok(header) = decode_header(token) else {
            return None;
        };

        if header.alg != Algorithm::HS256 {
            return None;
        }

        decode::<Value>(token, decoding_key, validation)
            .ok()
            .map(|data| data.claims)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PreviewClaims<'a> {
    exp: u64,
    scope: &'a str,
    channel: &'a str,
}

fn preview_path_channel(path: &str) -> Option<&str> {
    path.strip_prefix("/iptv/preview/")
        .and_then(|remaining| remaining.split('/').next())
        .filter(|channel| !channel.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, middleware, routing::get, Router};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde::Serialize;
    use tower::ServiceExt;

    use crate::host::state::AppState;

    #[derive(Debug, Serialize)]
    struct TestClaims {
        exp: usize,
    }

    fn signed_token(secret: &str, exp: usize) -> String {
        encode(
            &Header::default(),
            &TestClaims { exp },
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("encode jwt")
    }

    fn test_state(secret: Option<&str>) -> AppState {
        AppState::for_tests(IptvAccessTokenVerifier::from_optional_secret(
            secret.map(ToString::to_string),
        ))
    }

    async fn protected_status(state: AppState, uri: &str) -> StatusCode {
        let app = Router::new()
            .route("/iptv/channels.m3u", get(|| async { StatusCode::OK }))
            .route("/iptv/logos/gen", get(|| async { StatusCode::OK }))
            .route("/iptv/channel/{file}", get(|| async { StatusCode::OK }))
            .route(
                "/iptv/session/{channel}/{file}",
                get(|| async { StatusCode::OK }),
            )
            .route(
                "/iptv/preview/{channel}/{file}",
                get(|| async { StatusCode::OK }),
            )
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                require_iptv_access,
            ))
            .with_state(state);

        app.oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router response")
        .status()
    }

    #[test]
    fn empty_token_yields_empty_suffixes() {
        let token = AccessToken::new(None::<&str>);
        assert_eq!(token.query(), "");
        assert_eq!(token.query_amp(), "");
    }

    #[test]
    fn whitespace_token_is_treated_as_missing() {
        let token = AccessToken::new(Some("   "));
        assert_eq!(token.query(), "");
        assert_eq!(token.query_amp(), "");
    }

    #[test]
    fn present_token_formats_both_suffixes() {
        let token = AccessToken::new(Some("abc123"));
        assert_eq!(token.query(), "?access_token=abc123");
        assert_eq!(token.query_amp(), "&access_token=abc123");
    }

    #[test]
    fn verifier_is_disabled_without_secret() {
        let verifier = IptvAccessTokenVerifier::from_optional_secret(None);
        assert!(!verifier.is_enabled());
        assert!(verifier.verify("anything"));
    }

    #[test]
    fn verifier_accepts_valid_hs256_token() {
        let verifier = IptvAccessTokenVerifier::enabled("secret");
        let token = signed_token("secret", 4_102_444_800);
        assert!(verifier.verify(&token));
    }

    #[test]
    fn hs256_backend_preserves_independent_token_bytes_and_rejects_tampering() {
        // Fixed synthetic vector generated independently with Node.js crypto HMAC-SHA256.
        const TOKEN: &str = concat!(
            "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.",
            "eyJzdWIiOiJzeW50aGV0aWMtYmFja2VuZC1jb21wYXRpYmlsaXR5IiwiZXhwIjo0MTAyNDQ0ODAwfQ.",
            "Lex47f2ME0r0GQaHIMAhtQFlERRZALVuo039X4kmfW0",
        );
        #[derive(Serialize)]
        struct CompatibleClaims {
            sub: &'static str,
            exp: u64,
        }
        let claims = CompatibleClaims {
            sub: "synthetic-backend-compatibility",
            exp: 4_102_444_800,
        };
        let secret = b"synthetic-iptv-secret";
        assert_eq!(
            encode(
                &Header::default(),
                &claims,
                &EncodingKey::from_secret(secret)
            )
            .unwrap(),
            TOKEN
        );
        let verifier = IptvAccessTokenVerifier::enabled("synthetic-iptv-secret");
        assert!(verifier.verify(TOKEN));
        assert!(!IptvAccessTokenVerifier::enabled("different-synthetic-secret").verify(TOKEN));
        let (payload, signature) = TOKEN.rsplit_once('.').unwrap();
        let tampered = format!("{payload}.A{}", &signature[1..]);
        assert!(!verifier.verify(&tampered));
    }

    #[test]
    fn verifier_rejects_a_valid_token_using_another_hmac_algorithm() {
        let token = encode(
            &Header::new(Algorithm::HS384),
            &TestClaims { exp: 4_102_444_800 },
            &EncodingKey::from_secret(b"synthetic-iptv-secret"),
        )
        .unwrap();
        assert!(!IptvAccessTokenVerifier::enabled("synthetic-iptv-secret").verify(&token));
    }

    #[test]
    fn verifier_rejects_invalid_or_expired_tokens() {
        let verifier = IptvAccessTokenVerifier::enabled("secret");
        assert!(!verifier.verify("not-a-token"));

        let expired = signed_token("secret", 1);
        assert!(!verifier.verify(&expired));
    }

    #[tokio::test]
    async fn middleware_allows_requests_when_auth_disabled() {
        let status = protected_status(test_state(None), "/iptv/channels.m3u").await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn middleware_requires_valid_query_token_when_enabled() {
        let token = signed_token("secret", 4_102_444_800);
        let status = protected_status(
            test_state(Some("secret")),
            &format!("/iptv/channels.m3u?access_token={token}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn middleware_rejects_missing_token_when_enabled() {
        let status = protected_status(test_state(Some("secret")), "/iptv/channels.m3u").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn middleware_exempts_logo_paths() {
        let status = protected_status(test_state(Some("secret")), "/iptv/logos/gen").await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn preview_token_is_limited_to_its_channel_playlists_and_segments() {
        let state = test_state(Some("secret"));
        let token = state
            .iptv_access_token_verifier
            .issue_preview_token("12")
            .unwrap()
            .unwrap();

        for path in [
            "/iptv/preview/12/master.m3u8",
            "/iptv/preview/12/hls.m3u8",
            "/iptv/preview/12/live_sub.m3u8",
            "/iptv/preview/12/live000001.vtt",
            "/iptv/preview/12/segment-1.ts",
        ] {
            let status =
                protected_status(state.clone(), &format!("{path}?access_token={token}")).await;
            assert_eq!(status, StatusCode::OK, "{path}");
        }
        let other_channel = protected_status(
            state.clone(),
            &format!("/iptv/preview/13/master.m3u8?access_token={token}"),
        )
        .await;
        assert_eq!(other_channel, StatusCode::UNAUTHORIZED);
        for path in [
            "/iptv/preview/13/live_sub.m3u8",
            "/iptv/preview/13/live000001.vtt",
            "/iptv/session/12/live_sub.m3u8",
            "/iptv/session/12/live000001.vtt",
        ] {
            assert_eq!(
                protected_status(state.clone(), &format!("{path}?access_token={token}")).await,
                StatusCode::UNAUTHORIZED,
                "{path}"
            );
        }
        let normal_channel = protected_status(
            state.clone(),
            &format!("/iptv/channel/12.m3u8?access_token={token}"),
        )
        .await;
        assert_eq!(normal_channel, StatusCode::UNAUTHORIZED);
        let channel_list =
            protected_status(state, &format!("/iptv/channels.m3u?access_token={token}")).await;
        assert_eq!(channel_list, StatusCode::UNAUTHORIZED);
    }
}
