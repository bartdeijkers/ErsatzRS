use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde_json::Value;

use crate::state::{AppState, IptvAccessTokenVerifier};

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
        Some(token) if state.iptv_access_token_verifier.verify(&token) => next.run(request).await,
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
        }
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.validation.is_some() && self.decoding_key.is_some()
    }

    #[must_use]
    pub fn verify(&self, token: &str) -> bool {
        let (Some(validation), Some(decoding_key)) = (&self.validation, &self.decoding_key) else {
            return true;
        };

        let Ok(header) = decode_header(token) else {
            return false;
        };

        if header.alg != Algorithm::HS256 {
            return false;
        }

        decode::<Value>(token, decoding_key, validation).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, middleware, routing::get, Router};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde::Serialize;
    use tower::ServiceExt;

    use crate::state::AppState;

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
}
