//! EB1: actual Arabic G0 cell editing, distinct from generated G2 words.
use super::*;

const ARABIC47: Context = Context {
    name: "arabic47_g0",
    primary: 0x47,
    headline: "\u{fe8d}\u{fedf}\u{fed3}\u{fe8f}\u{fefb} 200",
    sentinels: "\u{fe83}\u{fefb} 200",
    from: 0x47,
    to: 0x48,
    // QTM1.0's Arabic Unicode keymap is empty. Its unchanged ASCII key path
    // selects this native Table44 code in the active Arabic G0 repertoire;
    // this proves cell editing, not Arabic Unicode keyboard support.
    typed: 'H',
    response: arabic47,
};

const ARABIC: [Context; 2] = [
    ARABIC47,
    Context {
        name: "arabic57_g0",
        primary: 0x57,
        response: arabic57,
        ..ARABIC47
    },
];

fn arabic47(theme: &str, page: u16, subcode: u16) -> Value {
    response(ARABIC[0], theme, page, subcode)
}

fn arabic57(theme: &str, page: u16, subcode: u16) -> Value {
    response(ARABIC[1], theme, page, subcode)
}

#[tokio::test]
async fn arabic_g0_cell_concepts_preview_publish_and_cold_reopen() {
    for context in ARABIC {
        Box::pin(run_with_publication(context, None, true)).await;
    }
}

#[tokio::test]
#[ignore = "requires two actual pinned headed QTM saves in fresh ERSATZRS_TELETEXT_ARABIC_G0_EDITOR_DIR"]
async fn actual_qtm_arabic_g0_cells_import_preview_publish_reopen() {
    let root = std::path::PathBuf::from(super::super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_ARABIC_G0_EDITOR_DIR",
    ));
    std::fs::create_dir(&root).expect("Arabic G0 editor destination must be new");
    for context in ARABIC {
        Box::pin(run_with_publication(
            context,
            Some(&root.join(context.name)),
            true,
        ))
        .await;
    }
}
