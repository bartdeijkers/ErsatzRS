//! Graphics frame rates retain Next's decimal or rational wire spelling.

use crate::FrameRate;

#[derive(Debug, Clone, PartialEq)]
pub struct GraphicsFrameRate {
    wire: String,
    parsed: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid graphics frame rate")]
pub struct InvalidGraphicsFrameRate;

impl GraphicsFrameRate {
    /// Parse upstream's unsigned decimal or positive `Int32` rational syntax.
    ///
    /// # Errors
    /// Rejects zero, non-finite, signed, overflowing or malformed rates.
    pub fn parse(wire: &str) -> Result<Self, InvalidGraphicsFrameRate> {
        let parsed = if let Some((numerator, denominator)) = wire.split_once('/') {
            let positive = |value: &str| {
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(InvalidGraphicsFrameRate);
                }
                value
                    .parse::<i32>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or(InvalidGraphicsFrameRate)
            };
            f64::from(positive(numerator)?) / f64::from(positive(denominator)?)
        } else {
            if wire.is_empty()
                || !wire
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'.')
            {
                return Err(InvalidGraphicsFrameRate);
            }
            wire.parse::<f64>().map_err(|_| InvalidGraphicsFrameRate)?
        };
        if !parsed.is_finite() || parsed <= 0.0 {
            return Err(InvalidGraphicsFrameRate);
        }
        Ok(Self {
            wire: wire.to_owned(),
            parsed,
        })
    }

    #[must_use]
    pub fn r_frame_rate(&self) -> String {
        self.wire.clone()
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.wire
    }

    #[must_use]
    pub const fn parsed(&self) -> f64 {
        self.parsed
    }
}

impl From<FrameRate> for GraphicsFrameRate {
    fn from(rate: FrameRate) -> Self {
        Self {
            wire: rate.r_frame_rate(),
            parsed: rate.parsed(),
        }
    }
}
