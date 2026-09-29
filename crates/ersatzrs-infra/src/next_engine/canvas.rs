//! Validated canvas request context from upstream `GetGraphicsCanvas` (ADR 0105).

use std::time::Duration;

#[path = "canvas_binding.rs"]
mod binding;
pub use binding::CanvasBinding;

const MAX_MILLISECONDS: u64 = i64::MAX.unsigned_abs() / 10_000;

/// Validated timing. Raw header values never escape in diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasRequest {
    offset: Duration,
    duration: Duration,
    frame_rate: String,
    parsed_frame_rate: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid graphics canvas timing headers")]
pub struct InvalidCanvasRequest;

impl CanvasRequest {
    /// Preserve Next's wire rate while validating upstream's millisecond bounds.
    /// The HTTP caller must reject duplicate headers before calling this method.
    ///
    /// # Errors
    /// Rejects invalid, zero-duration, non-finite or overflowing input.
    pub fn parse(offset: &str, duration: &str, rate: &str) -> Result<Self, InvalidCanvasRequest> {
        let offset = unsigned(offset)?;
        let duration = unsigned(duration)?;
        if offset > MAX_MILLISECONDS || duration == 0 || duration > MAX_MILLISECONDS - offset {
            return Err(InvalidCanvasRequest);
        }
        let parsed_frame_rate = parse_rate(rate)?;
        Ok(Self {
            offset: Duration::from_millis(offset),
            duration: Duration::from_millis(duration),
            frame_rate: rate.to_owned(),
            parsed_frame_rate,
        })
    }

    #[must_use]
    pub const fn offset(&self) -> Duration {
        self.offset
    }

    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    #[must_use]
    pub fn frame_rate(&self) -> &str {
        &self.frame_rate
    }

    #[must_use]
    pub const fn parsed_frame_rate(&self) -> f64 {
        self.parsed_frame_rate
    }
}

fn unsigned(value: &str) -> Result<u64, InvalidCanvasRequest> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(InvalidCanvasRequest);
    }
    value.parse().map_err(|_| InvalidCanvasRequest)
}

fn parse_rate(value: &str) -> Result<f64, InvalidCanvasRequest> {
    let rate = if let Some((numerator, denominator)) = value.split_once('/') {
        let numerator = i32::try_from(unsigned(numerator)?).map_err(|_| InvalidCanvasRequest)?;
        let denominator =
            i32::try_from(unsigned(denominator)?).map_err(|_| InvalidCanvasRequest)?;
        if numerator <= 0 || denominator <= 0 {
            return Err(InvalidCanvasRequest);
        }
        f64::from(numerator) / f64::from(denominator)
    } else {
        // NumberStyles.AllowDecimalPoint: no whitespace, signs or exponents.
        if value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
        {
            return Err(InvalidCanvasRequest);
        }
        value.parse::<f64>().map_err(|_| InvalidCanvasRequest)?
    };
    if !rate.is_finite() || rate <= 0.0 {
        return Err(InvalidCanvasRequest);
    }
    Ok(rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_request_preserves_wire_rate_and_checked_timing() {
        for rate in ["24000/1001", "25", "29.97", ".5", "25."] {
            let request = CanvasRequest::parse("44000", "30000", rate).unwrap();
            assert_eq!(request.offset(), Duration::from_secs(44));
            assert_eq!(request.duration(), Duration::from_secs(30));
            assert_eq!(request.frame_rate(), rate);
            assert!(request.parsed_frame_rate() > 0.0);
        }
        assert!(CanvasRequest::parse("0", &MAX_MILLISECONDS.to_string(), "25").is_ok());
        assert!(CanvasRequest::parse("1", &MAX_MILLISECONDS.to_string(), "25").is_err());
    }

    #[test]
    fn canvas_request_rejects_upstream_invalid_header_cases() {
        for rate in [
            "garbage",
            "25/0",
            "0/1",
            "-25/1",
            "25/1/1",
            "NaN",
            "Infinity",
            "0",
            "",
            " 25",
            "+25",
            "1e2",
            "1..2",
            "2147483648/1",
        ] {
            assert!(CanvasRequest::parse("0", "44000", rate).is_err(), "{rate}");
        }
        for value in [
            "-1",
            "+1",
            "1.0",
            "",
            " 1",
            "922337203685478",
            "18446744073709551616",
        ] {
            assert!(CanvasRequest::parse(value, "44000", "25").is_err());
            assert!(CanvasRequest::parse("0", value, "25").is_err());
        }
        assert!(CanvasRequest::parse("0", "0", "25").is_err());
    }
}
