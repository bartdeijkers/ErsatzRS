//! AB2 symbols retain their original scalar/context across the application flow.
use super::*;

const SYMBOL_CONTEXTS: [Context; 2] = [
    Context {
        name: "greek_numeral_rectangle",
        primary: 0x37,
        headline: "\u{0374}■ 200",
        sentinels: "α■ 200",
        from: 0x52,
        to: 0x61,
        typed: 'α',
        response: greek,
    },
    Context {
        name: "cyrillic_supplementary_alpha",
        primary: 0x24,
        headline: "Б 200",
        sentinels: "α■ 200",
        from: 0x42,
        to: 0x43,
        typed: 'Ц',
        response: cyrillic,
    },
];

fn greek(theme: &str, page: u16, subcode: u16) -> Value {
    response(SYMBOL_CONTEXTS[0], theme, page, subcode)
}
fn cyrillic(theme: &str, page: u16, subcode: u16) -> Value {
    response(SYMBOL_CONTEXTS[1], theme, page, subcode)
}

#[tokio::test]
async fn supplementary_symbols_keep_literal_scalars_and_context_through_publication() {
    for context in SYMBOL_CONTEXTS {
        Box::pin(run_with_publication(context, None, true)).await;
    }
}
