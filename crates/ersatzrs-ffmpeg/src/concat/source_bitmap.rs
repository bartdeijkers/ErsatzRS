//! Initialize a subtitle reader from already isolated zero-origin fragments.
//! Raw artifact inpoints can return earlier packets; isolation and source/clock
//! qualification belong to the retained preparer, not this recipe (ADR 0121).

use std::{
    fmt::{self, Write as _},
    path::Path,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SourceBitmapConcatError {
    #[error("caption fragment delay is invalid")]
    Delay,
    #[error("caption fragment locator is invalid")]
    Locator,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SourceBitmapConcat {
    bootstrap: String,
    following: Option<(String, i64)>,
}

impl fmt::Debug for SourceBitmapConcat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceBitmapConcat")
            .field(
                "following_delay_microseconds",
                &self.following.as_ref().map(|(_, delay)| delay),
            )
            .finish_non_exhaustive()
    }
}

impl SourceBitmapConcat {
    /// Render a complete initial display, followed by its finite future sequence.
    ///
    /// Both private fragments must already have zero-origin complete groups,
    /// with no seek preroll. The bootstrap may be an explicit empty display.
    /// The delay is the checked time until the first following group. The caller
    /// must qualify and retain the files, bind the source/AV clock and final
    /// command, and restrict native input protocols to local files. This plan
    /// grants no such authority or client support. It does not shorten packet
    /// durations: actual subsequent clear/update behavior requires qualification.
    ///
    /// # Errors
    /// Rejects nonpositive following delays and unrepresentable locators.
    pub fn new(
        bootstrap: &Path,
        following: Option<(&Path, i64)>,
    ) -> Result<Self, SourceBitmapConcatError> {
        let bootstrap = quoted(bootstrap)?;
        let following = following
            .map(|(path, delay)| {
                if delay <= 0 {
                    return Err(SourceBitmapConcatError::Delay);
                }
                Ok((quoted(path)?, delay))
            })
            .transpose()?;
        Ok(Self {
            bootstrap,
            following,
        })
    }

    /// Generate ffconcat without opening files or publishing caption content.
    #[must_use]
    pub fn render(&self) -> String {
        let mut script = format!("ffconcat version 1.0\nfile {}\n", self.bootstrap);
        if let Some((path, delay)) = &self.following {
            let _ = write!(
                script,
                "duration {}.{:06}\nfile {path}\n",
                delay / 1_000_000,
                delay % 1_000_000,
            );
        }
        script
    }
}

fn quoted(path: &Path) -> Result<String, SourceBitmapConcatError> {
    let path = path.to_str().ok_or(SourceBitmapConcatError::Locator)?;
    if path.is_empty() || path.len() > 4096 || path.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(SourceBitmapConcatError::Locator);
    }
    // Backslashes remain literal inside single quotes on every platform;
    // escape a quote outside the quoted segment, then resume quoting.
    Ok(format!("'{}'", path.replace('\'', "'\\''")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_bitmap_precedes_the_shifted_update_and_clear() {
        let plan = SourceBitmapConcat::new(
            Path::new("synthetic-active.mks"),
            Some((Path::new("synthetic-future.mks"), 7_560_000)),
        )
        .unwrap();
        insta::assert_snapshot!(plan.render());
        assert!(!format!("{plan:?}").contains("synthetic-active"));
    }

    #[test]
    fn boundaries_preserve_initial_delay_exact_groups_and_final_clear() {
        let empty = Path::new("synthetic-empty.mks");
        let before =
            SourceBitmapConcat::new(empty, Some((Path::new("synthetic-first.mks"), 621_933)))
                .unwrap();
        let exact =
            SourceBitmapConcat::new(Path::new("synthetic-update.mks"), Some((empty, 15_000_000)))
                .unwrap();
        let after = SourceBitmapConcat::new(empty, None).unwrap();
        insta::assert_snapshot!(format!(
            "before first\n{}\nexact update\n{}\nafter clear\n{}",
            before.render(),
            exact.render(),
            after.render()
        ));
    }

    #[test]
    fn invalid_coordinates_and_directive_injection_are_rejected() {
        let path = Path::new("caption.mks");
        for delay in [0, -1, i64::MIN] {
            assert_eq!(
                SourceBitmapConcat::new(path, Some((path, delay))),
                Err(SourceBitmapConcatError::Delay)
            );
        }
        for name in [
            "",
            "caption.mks\nfile other",
            "caption\rmks",
            "caption\0mks",
        ] {
            assert_eq!(
                SourceBitmapConcat::new(Path::new(name), None),
                Err(SourceBitmapConcatError::Locator)
            );
            assert_eq!(
                SourceBitmapConcat::new(path, Some((Path::new(name), 1))),
                Err(SourceBitmapConcatError::Locator)
            );
        }
        assert_eq!(
            SourceBitmapConcat::new(Path::new(&"a".repeat(4097)), None),
            Err(SourceBitmapConcatError::Locator)
        );
    }

    #[test]
    fn quoted_paths_keep_literal_quotes_and_backslashes() {
        let plan = SourceBitmapConcat::new(
            Path::new("synthetic assets/caption's\\body.mks"),
            Some((Path::new("future.mks"), 7_560_000)),
        )
        .unwrap();
        insta::assert_snapshot!(plan.render());
    }
}
