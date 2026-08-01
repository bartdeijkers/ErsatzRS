//! Shared channel-identifier helpers used by both the M3U playlist (`tvg-id`)
//! and the XMLTV guide (`<channel id>` / `<programme channel>`). Both outputs
//! must agree on the same identifier or IPTV clients that match strictly by
//! id (rather than falling back to channel number/name) cannot associate
//! guide data with a channel.

/// Numeric sort key for a channel number string; unparsable values sort last.
pub fn parse_channel_number(number: &str) -> f64 {
    number.parse::<f64>().unwrap_or(f64::MAX)
}

/// Ports `ErsatzTV.Core.Iptv.ChannelIdentifier.FromNumber`.
///
/// The `ErsatzTV` algorithm is `(int)(decimal.Parse(channelNumber) * 100)`,
/// i.e. channel numbers with at most two decimals (per
/// `Channel.NUMBER_PATTERN`). We mirror the cast by scaling to cents and
/// rounding, then saturating into `i32` so malformed input can never alias a
/// valid identifier.
pub fn channel_identifier(channel_number: &str) -> String {
    let scaled = (channel_number.parse::<f64>().unwrap_or(0.0) * 100.0).round();
    let mut number: i32 = if scaled.is_finite() {
        #[allow(clippy::cast_possible_truncation)] // clamped to i32 range above
        let truncated = scaled.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
        truncated
    } else {
        0
    };
    let mut id: i32 = 0;
    while number != 0 {
        id += (number % 10) + 48;
        number /= 10;
    }

    let instance_id = instance_id();
    if instance_id.is_empty() {
        format!("C{channel_number}.{id}.ersatztv.org")
    } else {
        format!("C{channel_number}.{id}.{instance_id}.ersatztv.org")
    }
}

fn instance_id() -> String {
    #[allow(clippy::disallowed_methods)]
    std::env::var("ETV_INSTANCE_ID").unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_identifier_matches_ersatztv_algorithm() {
        // ErsatzTV: number * 100, sum digits + 48
        // "1"   -> 100 -> (1+48)+(0+48)+(0+48) = 145
        // "1.5" -> 150 -> (1+48)+(5+48)+(0+48) = 150
        // "2"   -> 200 -> (2+48)+(0+48)+(0+48) = 146
        assert_eq!(channel_identifier("1"), "C1.145.ersatztv.org");
        assert_eq!(channel_identifier("1.5"), "C1.5.150.ersatztv.org");
        assert_eq!(channel_identifier("2"), "C2.146.ersatztv.org");
    }
}
