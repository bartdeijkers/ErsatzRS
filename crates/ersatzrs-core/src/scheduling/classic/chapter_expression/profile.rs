//! ADR 0098's deterministic root collation and invariant number conversion.
use super::{ExpressionError, Result};
use icu_collator::{
    options::{AlternateHandling, CaseLevel, CollatorOptions, Strength},
    Collator, CollatorBorrowed, CollatorPreferences,
};
use regex::Regex;
use std::{cmp::Ordering, sync::LazyLock};

static COLLATOR: LazyLock<Result<CollatorBorrowed<'static>>> = LazyLock::new(|| {
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Secondary);
    options.case_level = Some(CaseLevel::Off);
    options.alternate_handling = Some(AlternateHandling::NonIgnorable);
    // Empty preferences select the root locale, never the host/request locale.
    Collator::try_new(CollatorPreferences::default(), options).map_err(|_| ExpressionError)
});

pub(super) fn compare(left: &str, right: &str) -> Result<Ordering> {
    COLLATOR
        .as_ref()
        .map(|collator| collator.compare(left, right))
        .map_err(|_| ExpressionError)
}

// NumberStyles.Float | AllowThousands accepts grouping only before the decimal
// and exponent. It does not enforce three-digit group widths: "1,,2" and "1,"
// are valid, while ",1" and "1.2,3" are not. These are conversion rules for
// strings; commas in expression numeric literals remain argument separators.
static REAL: LazyLock<Result<Regex>> = LazyLock::new(|| {
    Regex::new(r"\A[+-]?(?:[0-9][0-9,]*(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?\z")
        .map_err(|_| ExpressionError)
});

pub(super) fn real(text: &str) -> Result<f64> {
    let text = numeric_text(text);
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    if unsigned.eq_ignore_ascii_case("nan") {
        return Ok(f64::NAN);
    }
    if unsigned.eq_ignore_ascii_case("infinity") {
        return Ok(if text.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    if !REAL.as_ref().map_err(|_| ExpressionError)?.is_match(text) {
        return Err(ExpressionError);
    }
    text.replace(',', "").parse().map_err(|_| ExpressionError)
}

/// .NET integer/real conversion accepts ASCII whitespace and trailing NULs;
/// non-ASCII spaces are not silently rewritten into valid numeric strings.
pub(super) fn numeric_text(text: &str) -> &str {
    text.trim_end_matches('\0')
        .trim_matches(|c: char| c == ' ' || ('\t'..='\r').contains(&c))
}

/// Invariant regex casing is simple BMP lowercase. Supplementary characters
/// remain their original UTF-16 surrogate pair; unlike string collation, .NET
/// regex does not case-fold supplementary letters or perform normalization.
pub(super) fn like_units(text: &str) -> Vec<u16> {
    let mut output = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let mut buffer = [0; 2];
        if ch.len_utf16() == 1 {
            let mut lower = ch.to_lowercase();
            let first = lower.next().unwrap_or(ch);
            // U+0130 expands under Unicode full lowercase but is unchanged by
            // invariant simple casing. Do not turn it into two regex units.
            let mapped = if lower.next().is_none() { first } else { ch };
            output.extend_from_slice(mapped.encode_utf16(&mut buffer));
        } else {
            output.extend_from_slice(ch.encode_utf16(&mut buffer));
        }
    }
    output
}
