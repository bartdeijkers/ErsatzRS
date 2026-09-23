//! Chapter filtering follows ErsatzTV's `FillerExpression.FilterChapters`.
//! Parsing uses a dedicated `NCalc` 5.12 grammar, rather than interpreting the
//! expression as a different scripting language.

mod evaluation;
#[cfg(test)]
mod tests;
mod value;

use chrono::Duration;
use ersatzrs_domain::MediaChapter;
use pest::Parser;
use pest_derive::Parser;
use value::Value;

#[derive(Parser)]
#[grammar = "scheduling/classic/chapter_expression/grammar.pest"]
struct ExpressionParser;

/// Errors deliberately exclude expressions, chapter titles and parser input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid chapter filler expression")]
pub(super) struct ExpressionError;

type Result<T> = std::result::Result<T, ExpressionError>;

struct Parameters<'a> {
    chapter: &'a MediaChapter,
    number: usize,
    total_points: usize,
    duration: f64,
    end: f64,
    last_filler: f64,
    matches: usize,
}

impl Parameters<'_> {
    fn get(&self, name: &str) -> Value {
        let point = self.chapter.end_time.as_seconds_f64();
        match name {
            "last_mid_filler" => Value::Real(point - self.last_filler),
            "total_points" => Value::integer(i64::try_from(self.total_points).unwrap_or(i64::MAX)),
            "total_duration" => Value::Real(self.duration),
            "total_progress" => Value::Real(point / self.end),
            "remaining_duration" => Value::Real(self.duration - point),
            "matched_points" => Value::integer(i64::try_from(self.matches).unwrap_or(i64::MAX)),
            "point" => Value::Real(point),
            "num" => Value::integer(i64::try_from(self.number).unwrap_or(i64::MAX)),
            "title" => Value::Text(self.chapter.title.clone()),
            // Upstream assigns e.Result even for unrecognised names. Its
            // ParameterArgs setter marks that null value as a handled result.
            _ => Value::Null,
        }
    }
}

pub(super) fn filter_chapters(
    expression: &str,
    chapters: &[MediaChapter],
    content_duration: Duration,
) -> Result<Vec<MediaChapter>> {
    if chapters.is_empty() || expression.trim().is_empty() {
        return Ok(chapters.to_vec());
    }
    // With one chapter upstream never constructs/evaluates an expression.
    if chapters.len() == 1 {
        return Ok(Vec::new());
    }
    validate_complexity(expression)?;
    let mut parsed =
        ExpressionParser::parse(Rule::root, expression).map_err(|_| ExpressionError)?;
    let root = parsed.next().ok_or(ExpressionError)?;
    let start = chapters
        .iter()
        .map(|c| c.start_time)
        .min()
        .ok_or(ExpressionError)?;
    let end = chapters
        .iter()
        .map(|c| c.end_time)
        .max()
        .ok_or(ExpressionError)?;
    let mut last_filler = start.as_seconds_f64() - 99_999.0;
    let mut selected = Vec::new();
    for (index, chapter) in chapters[..chapters.len() - 1].iter().enumerate() {
        let parameters = Parameters {
            chapter,
            number: index + 1,
            total_points: chapters.len() - 1,
            duration: content_duration.as_seconds_f64(),
            end: end.as_seconds_f64(),
            last_filler,
            matches: selected.len(),
        };
        // NCalc's `as bool? == true` does not coerce a numeric/string result.
        if matches!(
            evaluation::evaluate(root.clone(), &parameters)?,
            Value::Boolean(true)
        ) {
            last_filler = chapter.end_time.as_seconds_f64();
            selected.push(chapter.clone());
        }
    }
    if !selected.is_empty() {
        let mut current = start;
        for chapter in &mut selected {
            chapter.start_time = current;
            current = chapter.end_time;
        }
        selected.push(MediaChapter {
            start_time: current,
            end_time: end,
            ..MediaChapter::default()
        });
    }
    Ok(selected)
}

/// Bound recursive grammar work without changing pest's process-global call
/// limit (other expression parsers share that library).
fn validate_complexity(expression: &str) -> Result<()> {
    if expression.len() > 65_536 {
        return Err(ExpressionError);
    }
    let mut closing = None;
    let mut escaped = false;
    let mut depth = 0_u32;
    for ch in expression.chars() {
        if let Some(end) = closing {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' && matches!(end, '\'' | '"') {
                escaped = true;
            } else if ch == end {
                closing = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '#' => closing = Some(ch),
            '[' => closing = Some(']'),
            '{' => closing = Some('}'),
            '(' => {
                depth += 1;
                if depth > 32 {
                    return Err(ExpressionError);
                }
            }
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}
