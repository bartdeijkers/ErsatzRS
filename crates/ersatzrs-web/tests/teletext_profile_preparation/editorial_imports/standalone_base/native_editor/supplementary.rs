//! The new generated G2 context must survive actual application storage/export.
//! Genuine editor and ordinary-output requirements remain separately scoped.
use super::*;

#[path = "supplementary/generated_edit.rs"]
mod generated_edit;

const CONTEXTS_G2: [Context; 4] = [
    Context {
        sentinels: "£ŁD 200",
        response: cyrillic1,
        ..CONTEXTS[0]
    },
    Context {
        sentinels: "£ŁD 200",
        response: cyrillic2,
        ..CONTEXTS[1]
    },
    Context {
        sentinels: "£ŁD 200",
        response: cyrillic3,
        ..CONTEXTS[2]
    },
    Context {
        name: "greek_g2",
        primary: 0x37,
        headline: "Αθήνα 200",
        sentinels: "£aΊ 200",
        from: 0x41,
        to: 0x42,
        typed: 'Β',
        response: greek,
    },
];

fn cyrillic1(theme: &str, page: u16, subcode: u16) -> Value {
    response(CONTEXTS_G2[0], theme, page, subcode)
}
fn cyrillic2(theme: &str, page: u16, subcode: u16) -> Value {
    response(CONTEXTS_G2[1], theme, page, subcode)
}
fn cyrillic3(theme: &str, page: u16, subcode: u16) -> Value {
    response(CONTEXTS_G2[2], theme, page, subcode)
}
fn greek(theme: &str, page: u16, subcode: u16) -> Value {
    response(CONTEXTS_G2[3], theme, page, subcode)
}

#[tokio::test]
async fn native_g2_profile_export_concept_preview_reopen_preserves_generated_context() {
    for context in CONTEXTS_G2 {
        Box::pin(run(context, None)).await;
    }
}
