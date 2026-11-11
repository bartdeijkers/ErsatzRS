//! Chapter filtering follows ErsatzTV's `FillerExpression.FilterChapters`.
//! Parsing uses a dedicated `NCalc` 6.2 grammar, rather than interpreting the
//! expression as a different scripting language.

mod evaluation;
mod profile;
#[cfg(test)]
mod profile_tests;
mod temporal;
#[cfg(test)]
mod tests;
mod value;

use crate::scheduling::SchedulingFillerPreset;
use chrono::Duration;
use ersatzrs_domain::{EntityId, MediaChapter};
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
    fn get(&self, name: &str) -> Result<Value> {
        let point = self.chapter.end_time.as_seconds_f64();
        Ok(match name {
            "last_mid_filler" => Value::Real(point - self.last_filler),
            "total_points" => Value::integer(i64::try_from(self.total_points).unwrap_or(i64::MAX)),
            "total_duration" => Value::Real(self.duration),
            "total_progress" => Value::Real(point / self.end),
            "remaining_duration" => Value::Real(self.duration - point),
            "matched_points" => Value::integer(i64::try_from(self.matches).unwrap_or(i64::MAX)),
            "point" => Value::Real(point),
            "num" => Value::integer(i64::try_from(self.number).unwrap_or(i64::MAX)),
            "title" => Value::Text(self.chapter.title.clone()),
            // Upstream row289 rejects unknown callbacks. Do not disclose the
            // saved parameter text in diagnostics (ADR 0098).
            _ => return Err(ExpressionError),
        })
    }
}

/// ErsatzTV row289's no-points result retains the original content span for
/// padding. The strict evaluator still rejects unsupported or excessive input.
pub(super) fn filter_chapters_or_fallback(
    preset: &SchedulingFillerPreset,
    schedule_item_id: EntityId,
    chapters: &[MediaChapter],
    content_duration: Duration,
    warnings: &mut Vec<String>,
) -> Vec<MediaChapter> {
    if let Ok(filtered) = filter_chapters(
        preset.expression.as_deref().unwrap_or_default(),
        chapters,
        content_duration,
    ) {
        return filtered;
    }
    let warning = format!(
        "invalid chapter filler expression for Classic filler preset {} ({}) on schedule item {}; using full-content fallback without internal chapter points",
        preset.id, preset.name, schedule_item_id
    );
    if !warnings.contains(&warning) {
        warnings.push(warning);
    }
    // Evaluation errors require multiple chapters; empty/single inputs
    // bypass evaluation. Keep this total even if that policy changes.
    match (
        chapters.iter().map(|chapter| chapter.start_time).min(),
        chapters.iter().map(|chapter| chapter.end_time).max(),
    ) {
        (Some(start_time), Some(end_time)) => vec![MediaChapter {
            start_time,
            end_time,
            ..MediaChapter::default()
        }],
        _ => Vec::new(),
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
    validate_literals(root.clone())?;
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

/// Parsing validates every literal, including literals in an untaken lazy
/// branch. Function calls and parameter evaluation remain lazy at runtime.
fn validate_literals(root: pest::iterators::Pair<'_, Rule>) -> Result<()> {
    let mut pending = vec![root];
    while let Some(pair) = pending.pop() {
        match pair.as_rule() {
            Rule::string => {
                value::unescape(pair.as_str())?;
            }
            Rule::number => {
                Value::number(pair.as_str())?;
            }
            Rule::temporal => {
                Value::temporal(pair.as_str())?;
            }
            Rule::guid => {
                uuid::Uuid::parse_str(pair.as_str()).map_err(|_| ExpressionError)?;
            }
            _ => {}
        }
        pending.extend(pair.into_inner());
    }
    Ok(())
}
