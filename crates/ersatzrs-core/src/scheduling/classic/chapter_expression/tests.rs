use super::*;

pub(super) fn chapters() -> Vec<MediaChapter> {
    (0..6)
        .map(|index| MediaChapter {
            chapter_id: index + 1,
            start_time: Duration::minutes(index * 5),
            end_time: Duration::minutes((index + 1) * 5),
            title: if index % 2 == 0 { "Not Here" } else { "Here" }.into(),
            ..MediaChapter::default()
        })
        .collect()
}

fn ends(expression: &str) -> Vec<i64> {
    filter_chapters(expression, &chapters(), Duration::minutes(30))
        .unwrap()
        .iter()
        .map(|chapter| chapter.end_time.num_minutes())
        .collect()
}

#[test]
fn upstream_filler_expression_fixtures() {
    for expression in [
        "(point > 5 * 60) and (last_mid_filler > 5 * 60) and (matched_points < 2)",
        "(total_progress >= 0.2 and matched_points = 0) or (total_progress >= 0.6 and matched_points = 1)",
        "title == 'here'",
        "title != 'not here'",
    ] {
        assert_eq!(ends(expression), [10, 20, 30], "{expression}");
    }
    assert_eq!(ends("title like \"%here%\""), [5, 10, 15, 20, 25, 30]);
}

#[test]
fn filtered_chapters_merge_rejected_intervals_and_append_unlabelled_tail() {
    let result = filter_chapters("num = 2 or num = 4", &chapters(), Duration::minutes(30)).unwrap();
    assert_eq!(
        result
            .iter()
            .map(|c| (c.start_time.num_minutes(), c.end_time.num_minutes()))
            .collect::<Vec<_>>(),
        [(0, 10), (10, 20), (20, 30)]
    );
    assert_eq!(result[0].chapter_id, 2);
    assert_eq!(result[1].title, "Here");
    assert_eq!(result[2].chapter_id, 0);
    assert!(result[2].title.is_empty());
}

#[test]
fn all_callback_parameters_use_absolute_source_seconds_and_state() {
    let input = chapters()
        .into_iter()
        .map(|mut chapter| {
            chapter.start_time += Duration::seconds(100);
            chapter.end_time += Duration::seconds(100);
            chapter
        })
        .collect::<Vec<_>>();
    let expression = "total_points = 5 and total_duration = 1800 and remaining_duration = 1800-point and total_progress = point/1900 and (num = 2 and matched_points = 0 and last_mid_filler = 100599 or num = 4 and matched_points = 1 and last_mid_filler = 600)";
    let result = filter_chapters(expression, &input, Duration::minutes(30)).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].start_time, Duration::seconds(100));
    assert_eq!(result[1].end_time, Duration::seconds(1300));
}

#[test]
fn bypasses_and_non_boolean_results_match_upstream() {
    let input = chapters();
    assert_eq!(
        filter_chapters(" ", &input, Duration::minutes(30)).unwrap(),
        input
    );
    assert!(filter_chapters("broken !@", &[], Duration::zero())
        .unwrap()
        .is_empty());
    assert!(filter_chapters("broken !@", &input[..1], Duration::zero())
        .unwrap()
        .is_empty());
    for expression in ["false", "1", "'true'", "unknown"] {
        assert!(ends(expression).is_empty());
    }
    assert!(filter_chapters("num >", &input, Duration::zero()).is_err());
    assert!(filter_chapters("MissingFunction(1)", &input, Duration::zero()).is_err());
}

pub(super) fn value(expression: &str) -> Result<Value> {
    let chapter = &chapters()[1];
    let parameters = Parameters {
        chapter,
        number: 2,
        total_points: 5,
        duration: 1800.0,
        end: 1800.0,
        last_filler: -99999.0,
        matches: 0,
    };
    let pair = ExpressionParser::parse(Rule::root, expression)
        .map_err(|_| ExpressionError)?
        .next()
        .ok_or(ExpressionError)?;
    validate_literals(pair.clone())?;
    evaluation::evaluate(pair, &parameters)
}

#[test]
fn ncalc_operator_precedence_and_short_circuiting() {
    for expression in [
        "1 + 2 * 3 = 7",
        "-2 ** 2 = -4",
        "2 ** 3 ** 2 = 512",
        "5 / 2 = 2.5",
        "(3!) = 6",
        "not false AND true OR false",
        "false and MissingFunction() or true",
        "true or MissingFunction()",
        "true ? 1 = 1 : MissingFunction()",
        "if(true, 1, MissingFunction()) = 1",
        "ifs(false,MissingFunction(),true,2,MissingFunction()) = 2",
        "(5 & 3) = 1",
        "(1 << 3) = 8",
        "0x10 + 0o10 + 0b10 = 26",
        "[num] = 2 and {point} = 600",
        "unknown = other_unknown",
        "unknown + 1 = missing",
        "2147483647 + 1 = -2147483648",
        "9007199254740993 <> 9007199254740992",
    ] {
        assert_eq!(value(expression), Ok(Value::Boolean(true)), "{expression}");
    }
}

#[test]
fn ncalc_membership_wildcards_and_builtin_math() {
    for expression in [
        "title like 'h_re'",
        "title not like 'no%'",
        "title in ('else', 'HERE')",
        "num in (1,2,3)",
        "num not in (1,3)",
        "in(title,'HERE')",
        "'Here' in 'Something Here'",
        "not ('here' in 'Something Here')",
        "Round(2.5,0) = 2",
        "Round(3.5,0) = 4",
        "Abs(-2) = 2",
        "Floor(2.9) = 2 and Ceiling(2.1) = 3",
        "Pow(3,2) = 9 and Sqrt(9) = 3",
        "Min(2,3) = 2 and Max(2,3) = 3",
        "Sign(-2) = -1",
        "'hello ' + 'WORLD' = 'HELLO world'",
        "'1' + '2' = 3",
        "#1/1/2020# < #1/2/2020#",
        "#01:00:00# > #00:30:00#",
    ] {
        assert_eq!(value(expression), Ok(Value::Boolean(true)), "{expression}");
    }
}

/// Captured from `NCalcSync` 6.2.0 with `CaseInsensitiveStringComparer`, Linux
/// invariant culture, and callback num=2, point=600.0, title="Here"; other
/// callback names return null. This is a behavioral oracle, not a snapshot of
/// this evaluator's own output. ADR 0098 differences have separate profile tests.
#[test]
fn differential_ncalc_6_2_fixtures() {
    #[derive(serde::Deserialize)]
    struct Fixture {
        expression: String,
        value: Option<String>,
        #[serde(rename = "type")]
        kind: Option<String>,
        error: Option<String>,
    }
    let fixtures: Vec<Fixture> =
        serde_json::from_str(include_str!("ncalc_6_2_fixtures.json")).unwrap();
    let mut differences = Vec::new();
    for fixture in fixtures {
        let actual = value(&fixture.expression);
        if fixture.error.is_some() {
            if actual.is_ok() {
                differences.push(format!(
                    "{}: expected error, got {actual:?}",
                    fixture.expression
                ));
            }
            continue;
        }
        let expected = match fixture.kind.as_deref() {
            Some("Boolean") => Value::Boolean(fixture.value.as_deref() == Some("True")),
            Some("Int32") => Value::Int(fixture.value.unwrap().parse().unwrap()),
            Some("Int64") => Value::Long(fixture.value.unwrap().parse().unwrap()),
            Some("UInt64") => Value::Unsigned(fixture.value.unwrap().parse().unwrap()),
            Some("Double") => Value::Real(fixture.value.unwrap().parse().unwrap()),
            Some("String") => Value::Text(fixture.value.unwrap()),
            None => Value::Null,
            other => panic!("unsupported fixture kind {other:?}"),
        };
        let equal = match (&actual, &expected) {
            (Ok(Value::Real(a)), Value::Real(b)) if a.is_nan() && b.is_nan() => true,
            (Ok(Value::Real(a)), Value::Real(b)) if *a == 0.0 && *b == 0.0 => {
                a.to_bits() == b.to_bits()
            }
            _ => actual.as_ref() == Ok(&expected),
        };
        if !equal {
            differences.push(format!(
                "{}: expected {expected:?}, got {actual:?}",
                fixture.expression
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn ncalc_6_2_nan_comparisons_follow_ieee_754() {
    for expression in [
        "0/0 = 0/0",
        "0/0 = 1",
        "0/0 < -1/0",
        "0/0 <= -1/0",
        "0/0 > 1",
        "0/0 >= 1",
    ] {
        assert_eq!(value(expression), Ok(Value::Boolean(false)), "{expression}");
    }
    assert_eq!(value("0/0 != 0/0"), Ok(Value::Boolean(true)));
    assert_eq!(value("0/0 <> 1"), Ok(Value::Boolean(true)));
    assert!(matches!(value("IEEERemainder(-0.0,0)"), Ok(Value::Real(number)) if number.is_nan()));
    assert!(matches!(value("IEEERemainder(-0.0,0/0)"), Ok(Value::Real(number)) if number.is_nan()));
}

#[test]
fn expression_complexity_is_bounded_before_recursive_parsing() {
    let boundary = format!("{}true{}", "(".repeat(32), ")".repeat(32));
    assert_eq!(
        filter_chapters(&boundary, &chapters(), Duration::minutes(30))
            .unwrap()
            .len(),
        6
    );
    let chain = "1+".repeat(4000) + "1 = 4001";
    assert_eq!(
        filter_chapters(&chain, &chapters(), Duration::minutes(30))
            .unwrap()
            .len(),
        6
    );
    let nested_function = "if(true,".repeat(32) + "true" + &",false)".repeat(32);
    assert_eq!(
        filter_chapters(&nested_function, &chapters(), Duration::minutes(30))
            .unwrap()
            .len(),
        6
    );
    let malformed = "(".repeat(32) + "true";
    assert!(filter_chapters(&malformed, &chapters(), Duration::minutes(30)).is_err());
    let bare_ternary = "true?1:".repeat(4000) + "1";
    assert!(filter_chapters(&bare_ternary, &chapters(), Duration::minutes(30)).is_err());
    let deep = format!("{}true{}", "(".repeat(33), ")".repeat(33));
    let long = " ".repeat(65_537) + "true";
    for expression in [deep, long] {
        assert!(filter_chapters(&expression, &chapters(), Duration::minutes(30)).is_err());
    }
    assert!(validate_complexity("title = '(' and [unknown(] = missing").is_ok());
    assert!(validate_complexity("title = '\\'('").is_ok());
    assert_eq!(value("(34!) = 0"), Ok(Value::Boolean(true)));
    assert_eq!(value("(66!) = 0"), Ok(Value::Boolean(true)));
}
