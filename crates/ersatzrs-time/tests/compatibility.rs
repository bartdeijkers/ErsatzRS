use ersatzrs_time::{
    DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, Timelike, Utc,
};

#[test]
fn sqlite_and_wire_text_preserve_offset_and_fraction() {
    let cases = [
        ("0000-01-01T01:00:00+01:00", "0000-01-01T00:00:00Z"),
        ("9999-12-31T22:59:59-01:00", "9999-12-31T23:59:59Z"),
        (
            "2026-11-17T13:14:15.1234567Z",
            "2026-11-17T13:14:15.123456700Z",
        ),
        ("2026-11-17t13:14:15.12z", "2026-11-17T13:14:15.120Z"),
        ("2017-01-01 00:59:60.125+01:00", "2016-12-31T23:59:60.125Z"),
        (
            "1969-12-31T23:59:59.999999999Z",
            "1969-12-31T23:59:59.999999999Z",
        ),
    ];
    for (input, expected) in cases {
        let time = DateTime::parse_from_rfc3339(input).unwrap().to_utc();
        let expected_json = format!("\"{expected}\"");
        assert_eq!(serde_json::to_string(&time).unwrap(), expected_json);
        assert_eq!(
            serde_json::from_str::<DateTime<Utc>>(&expected_json).unwrap(),
            time
        );
    }

    let time: DateTime<Utc> = "2026-11-17T13:14:15.1234567Z".parse().unwrap();
    assert_eq!(
        time.format("%Y%m%d%H%M%S %z").to_string(),
        "20261117131415 +0000"
    );
    assert_eq!(
        time.naive_utc().format("%F %T%.f").to_string(),
        "2026-11-17 13:14:15.123456700"
    );
    assert_eq!(
        time.format("%Y%m%dT%H%M%S%fZ").to_string(),
        "20261117T131415123456700Z"
    );
    let historic_offset = FixedOffset::east_opt(1172).unwrap();
    assert_eq!(historic_offset.to_string(), "+00:19:32");
    assert_eq!(
        time.with_timezone(&historic_offset).to_rfc3339(),
        "2026-11-17T13:33:47.123456700+00:20"
    );
    let old = NaiveDateTime::parse_from_str("2026-11-17 13:14:15.1234567", "%F %T%.f").unwrap();
    assert_eq!(old.and_utc(), time);

    for invalid in [
        "2026-11-17T13:14:15",
        "2026-11-17T13:14:15+25:00",
        "2026-11-17T13:14:15.+00:00",
        "+10000-01-01T00:00:00Z",
    ] {
        assert!(DateTime::parse_from_rfc3339(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn signed_duration_encoding_and_arithmetic_keep_existing_bounds() {
    for (wire, duration) in [
        ("[0,0]", Duration::zero()),
        ("[-1,999999999]", -Duration::nanoseconds(1)),
        ("[1,250000000]", Duration::milliseconds(1250)),
        ("[9223372036854775,807000000]", Duration::MAX),
    ] {
        assert_eq!(serde_json::from_str::<Duration>(wire).unwrap(), duration);
        assert_eq!(serde_json::to_string(&duration).unwrap(), wire);
    }
    assert!(serde_json::from_str::<Duration>("[0,1000000000]").is_err());
    assert!(Duration::MAX
        .checked_add(&Duration::nanoseconds(1))
        .is_none());
    assert_eq!(
        (DateTime::UNIX_EPOCH - Duration::nanoseconds(1)).timestamp_millis(),
        -1
    );
    let leap = "2016-12-31T23:59:60.125Z".parse::<DateTime<Utc>>().unwrap();
    assert_eq!(
        (leap + Duration::milliseconds(900)).to_rfc3339(),
        "2017-01-01T00:00:00.025+00:00"
    );
    assert_eq!(leap.nanosecond(), 1_125_000_000);
}

#[test]
fn extended_sentinels_and_civil_month_rollover_survive() {
    assert_eq!(DateTime::MIN_UTC.timestamp(), -8_334_601_228_800);
    assert_eq!(DateTime::MAX_UTC.timestamp(), 8_210_266_876_799);
    assert_eq!(DateTime::MIN_UTC.year(), -262_143);
    assert_eq!(DateTime::MAX_UTC.year(), 262_142);
    assert!(DateTime::MAX_UTC
        .checked_add_signed(Duration::nanoseconds(1))
        .is_none());
    let day = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
    assert_eq!(
        day.checked_add_months(ersatzrs_time::Months::new(1))
            .unwrap()
            .to_string(),
        "2024-02-29"
    );
}
