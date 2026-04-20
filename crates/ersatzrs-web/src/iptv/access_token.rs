/// Holder for a propagated `?access_token=` query parameter.
///
/// Ports the `accessTokenUri`/`accessTokenUriAmp` pattern from
/// `ErsatzTV.Core/Iptv/ChannelPlaylist.cs` and `ErsatzTV/Controllers/IptvController.cs`:
/// the first URL on a page uses `?access_token=…`; subsequent URIs that
/// already carry a query string use `&access_token=…`.
///
/// Token **verification** (`ErsatzTV`'s `ConditionalIptvAuthorizeFilter`) is not
/// in scope for this slice — we only propagate what the caller supplies.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
