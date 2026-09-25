//! ADR 0098 expectations are intentionally separate from the retained `NCalc`
//! oracle. Run this same corpus in separately launched culture environments;
//! no test mutates process-global environment or locale state.
use super::{
    filter_chapters, profile,
    tests::{chapters, value},
    value::Value,
    ExpressionError,
};
use chrono::Duration;

#[test]
fn deterministic_profile_is_independent_of_host_culture() {
    // These are the accepted product expectations. In NCalc the I/i and comma
    // cases change under tr-TR/nl-NL; the four external process runs must not.
    for expression in [
        "'é' = 'e\u{0301}'",
        "'Ä' < 'Z'",
        "'I' = 'i'",
        "'I' <> 'ı'",
        "'i' <> 'İ'",
        "'é' <> 'e'",
        "'a-b' <> 'ab'",
        "'É' in ('e\u{0301}', 'other')",
        "in('É', 'e\u{0301}')",
        "not ('É' in 'e\u{0301}')",
        "'1,000' + 0 = 1000",
        "'1.000' + 0 = 1",
        "'1,5' + 0 = 15",
        "'1,000.5' + 0 = 1000.5",
        "#01/02/2020# < #02/01/2020#",
        "#2020-01-02# = #1/2/2020#",
        "#2020-01-02T03:04:05.1234567# = #1/2/2020 3:04:05.1234567#",
    ] {
        assert_eq!(value(expression), Ok(Value::Boolean(true)), "{expression}");
    }
    for expression in [
        "'1.000,5' + 0",
        "#01-02-2020#",
        "#01.02.2020#",
        "#00:00:01,5#",
    ] {
        assert_eq!(value(expression), Err(ExpressionError), "{expression}");
    }
}

#[test]
fn collation_selects_the_same_source_intervals_for_canonical_titles() {
    let mut input = chapters();
    input[1].title = "CAFÉ".into();
    input[3].title = "cafe\u{0301}".into();
    let result = filter_chapters("title = 'Café'", &input, Duration::minutes(30)).unwrap();
    assert_eq!(
        result
            .iter()
            .map(|c| (c.start_time.num_minutes(), c.end_time.num_minutes()))
            .collect::<Vec<_>>(),
        [(0, 10), (10, 20), (20, 30)]
    );
    assert_eq!(result[0].chapter_id, 2);
    assert_eq!(result[1].chapter_id, 4);
}

#[test]
fn invariant_numeric_strings_keep_real_and_integer_conversion_distinct() {
    for expression in [
        "'1,,000' + 0 = 1000",
        "'1,' + 0 = 1",
        "'1,2,3' + 0 = 123",
        "'1,000e2' + 0 = 100000",
        "'  +1,000.5  ' + 0 = 1000.5",
        "'.5' + 0 = 0.5",
        "'1.' + 0 = 1",
        "'1,000' + '2' = 1002",
        "'  +10  ' = 10",
        "'1\\u0000' + 0 = 1",
        "'1\\u0000' = 1",
        "'Infinity' + 0 = 1/0",
        "'-Infinity' + 0 = -1/0",
        "'1e999' + 0 = 1/0",
        "Round(1.5, '0') = 2",
        "(1 << '2') = 4",
    ] {
        assert_eq!(value(expression), Ok(Value::Boolean(true)), "{expression}");
    }
    for expression in ["'+NaN' + 0", "'nan' + 0"] {
        assert!(matches!(value(expression), Ok(Value::Real(number)) if number.is_nan()));
    }
    for expression in [
        "',1' + 0",
        "'1.0,0' + 0",
        "'1e1,0' + 0",
        "'inf' + 0",
        "'\\u00a01\\u00a0' + 0",
        "'1\\u0000x' + 0",
        "'1,000' = 1000",
        "'1.0' = 1",
        "Round(1.5, '0.0')",
        "1 << '1,0'",
        "'1,000' & 1",
        "'1.5' = 2",
    ] {
        assert_eq!(value(expression), Err(ExpressionError), "{expression}");
    }
}

#[test]
fn closed_temporal_profile_preserves_tick_precision_and_gregorian_fields() {
    for expression in [
        "#0001-01-01# = #1/1/0001#",
        "#9999-12-31T23:59:59.9999999# > #9999-12-31T23:59:59.9999998#",
        "#2024-02-29# = #2/29/2024#",
        "#00:00:00.0000001# > #00:00:00#",
        "#00:00:01.5# = #0:00:01.5000000#",
        "#25:00:00# > #24:59:59.9999999#",
        "#256204778:48:05.4775807# > #256204778:48:05.4775806#",
        "#1/2/2020 3:04:05# = #2020-01-02T03:04:05#",
    ] {
        assert_eq!(value(expression), Ok(Value::Boolean(true)), "{expression}");
    }
    for literal in [
        "#0000-01-01#",
        "#10000-01-01#",
        "#2020-00-01#",
        "#2020-01-00#",
        "#2023-02-29#",
        "#2020-04-31#",
        "#1/2/20#",
        "#1/2/020#",
        "#001/2/2020#",
        "#2020-1-02#",
        "#2020-01-2#",
        "#2020-01-01T24:00:00#",
        "#2020-01-01T1:00:00#",
        "#2020-01-01 01:00:00#",
        "#1/1/2020T01:00:00#",
        "#2020-01-01T00:00:00Z#",
        "#2020-01-01T00:00:00+01:00#",
        "#-1:00:00#",
        "#+1:00:00#",
        "#1:0:00#",
        "#1:00:0#",
        "#1:60:00#",
        "#1:00:60#",
        "#1:00:00.#",
        "#1:00:00.12345678#",
        "#256204778:48:05.4775808#",
        "#99999999999999999999:00:00#",
        "# 1:00:00#",
        "#1:00:00 #",
        "#January 1, 2020#",
    ] {
        assert_eq!(value(literal), Err(ExpressionError), "{literal}");
    }
}

#[test]
fn invariant_like_keeps_utf16_casing_separate_from_root_collation() {
    for expression in [
        "'I' like 'i'",
        "not ('I' like 'ı')",
        "not ('i' like 'İ')",
        "'ẞ' like 'ß'",
        "'\u{212A}' like 'k'",
        "not ('S' like 'ſ')",
        "not ('Σ' like 'ς')",
        "not ('𐐀' like '𐐨')",
        "not ('é' like 'e\u{0301}')",
        "not ('😀' like '_')",
        "'😀' like '__'",
        "'here\\n' like 'here'",
        "not ('a\\nb' like 'a%b')",
        "'\\uD83D\\uDE00' like '__'",
    ] {
        assert_eq!(value(expression), Ok(Value::Boolean(true)), "{expression}");
    }
}

#[test]
fn unsupported_utf16_and_boundary_failures_remain_redacted_errors() {
    for expression in ["'\\uD800'", "'\\uDC00'", "'\\uD800x'", "'\\uD800\\u0041'"] {
        assert_eq!(
            filter_chapters(expression, &chapters(), Duration::minutes(30)),
            Err(ExpressionError)
        );
    }
    let exactly_limit = format!("true{}", " ".repeat(65_532));
    assert_eq!(exactly_limit.len(), 65_536);
    assert_eq!(
        filter_chapters(&exactly_limit, &chapters(), Duration::minutes(30))
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        filter_chapters(&(exactly_limit + " "), &chapters(), Duration::minutes(30)),
        Err(ExpressionError)
    );
    let unicode_over_limit = format!("'{}'", "é".repeat(32_768));
    assert_eq!(
        filter_chapters(&unicode_over_limit, &chapters(), Duration::minutes(30)),
        Err(ExpressionError)
    );
    assert_eq!(
        ExpressionError.to_string(),
        "invalid chapter filler expression"
    );
}

#[test]
fn invariant_numeric_conversion_does_not_consult_environment() {
    assert_eq!(profile::real("1,234.5"), Ok(1234.5));
    assert_eq!(profile::real("1.234,5"), Err(ExpressionError));
}

#[test]
fn unsupported_literals_fail_even_in_untaken_lazy_branches() {
    for expression in [
        "false and '\\uD800' = ''",
        "if(true, true, #2023-02-29#)",
        "true or #01-02-2020# = #1/2/2020#",
        "false and '\\q' = ''",
    ] {
        assert_eq!(
            filter_chapters(expression, &chapters(), Duration::minutes(30)),
            Err(ExpressionError)
        );
    }
    // This is parser validation, not eager evaluation of function calls.
    assert_eq!(
        filter_chapters(
            "if(true, true, MissingFunction())",
            &chapters(),
            Duration::minutes(30)
        )
        .unwrap()
        .len(),
        6
    );
}
