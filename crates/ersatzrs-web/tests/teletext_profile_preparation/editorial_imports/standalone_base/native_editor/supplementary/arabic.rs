//! Arabic G0 sentinels survive a generated-G2 edit under both exact designations.
use super::*;

const ARABIC47: Context = Context {
    name: "arabic47_g2",
    primary: 0x47,
    headline: "\u{fe8d}\u{fedf}\u{fed3}\u{fe8f}\u{fefb} 200",
    sentinels: "\u{fe83}\u{fefb} 200",
    from: 0x47,
    to: 0x46,
    typed: 'F',
    response: arabic47,
};

pub(super) const CONTEXTS_ARABIC: [Context; 2] = [
    ARABIC47,
    Context {
        name: "arabic57_g2",
        primary: 0x57,
        response: arabic57,
        ..ARABIC47
    },
];

fn arabic47(theme: &str, page: u16, subcode: u16) -> Value {
    response(CONTEXTS_ARABIC[0], theme, page, subcode)
}
fn arabic57(theme: &str, page: u16, subcode: u16) -> Value {
    response(CONTEXTS_ARABIC[1], theme, page, subcode)
}
