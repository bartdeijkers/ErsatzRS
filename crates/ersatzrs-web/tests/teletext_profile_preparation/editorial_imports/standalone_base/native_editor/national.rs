//! Newly qualified native aliases must survive the existing application flow.
use super::*;

const NATIONAL_CONTEXTS: [Context; 3] = [
    Context {
        name: "polish",
        primary: 0x08,
        headline: "Ƶ 200",
        sentinels: "Ƶ 200",
        from: 0x5b,
        to: 0x5d,
        typed: 'Ł',
        response: polish,
    },
    Context {
        name: "serbian",
        primary: 0x1d,
        headline: "Đ 200",
        sentinels: "Đđ 200",
        from: 0x5d,
        to: 0x7d,
        typed: 'đ',
        response: serbian,
    },
    Context {
        name: "romanian",
        primary: 0x1f,
        headline: "Ă 200",
        sentinels: "Ăă 200",
        from: 0x5d,
        to: 0x7d,
        typed: 'ă',
        response: romanian,
    },
];

fn polish(theme: &str, page: u16, subcode: u16) -> Value {
    response(NATIONAL_CONTEXTS[0], theme, page, subcode)
}
fn serbian(theme: &str, page: u16, subcode: u16) -> Value {
    response(NATIONAL_CONTEXTS[1], theme, page, subcode)
}
fn romanian(theme: &str, page: u16, subcode: u16) -> Value {
    response(NATIONAL_CONTEXTS[2], theme, page, subcode)
}

#[tokio::test]
async fn qualified_national_profiles_export_import_preview_publish_and_reopen() {
    for context in NATIONAL_CONTEXTS {
        Box::pin(run_with_publication(context, None, true)).await;
    }
}
