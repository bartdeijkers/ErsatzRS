//! Complete factory provenance for generated Base context and authored Main.

use super::*;
use crate::teletext::authored_service_tests::declaration_limits;

fn original(
    primary: u8,
    text: &str,
    main: Option<Vec<TeletextEnhancementCommand>>,
) -> TeletextDeclaredService {
    let mut draft = input();
    draft.base.pages.truncate(1);
    draft.pages.truncate(1);
    draft.objects.clear();
    draft.drcs.clear();
    draft.magazines.clear();
    draft.base.pages[0].national_option = TeletextNationalOption::new(primary & 7).unwrap();
    draft.base.pages[0].rows[0].runs[0].content = TeletextRunContent::Text { text: text.into() };
    let mut presentation = super::super::presentation::western_latin_v1_presentation(
        TeletextNationalOption::new(primary & 7).unwrap(),
    )
    .unwrap();
    presentation.primary = TeletextPageCharsetDesignation::new(primary).unwrap();
    draft.pages[0].presentation = Some(presentation);
    draft.pages[0].local_program =
        main.map(|commands| TeletextEnhancementProgram::new(commands).unwrap());
    draft.pages[0].resources.objects = TeletextResolvedObjectPageLinks {
        public: None,
        global: None,
    };
    draft.pages[0].resources.defaults = [None; 4];
    draft.pages[0].resources.drcs = TeletextDrcsPageLinks::unlinked();
    let service = TeletextSemanticService::new(draft, semantic_limits()).unwrap();
    let owner = service.base().pages[0].address;
    TeletextDeclaredService::new(
        service,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner,
            mot: None,
        }],
        vec![],
        declaration_limits(),
    )
    .unwrap()
}

fn designated_main(designation: u8) -> Vec<TeletextEnhancementCommand> {
    vec![
        position(1, 0),
        TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(designation).unwrap(),
            },
        },
        glyph(9),
    ]
}

#[test]
fn native_factory_retains_exact_base_context_sources_and_optional_main_identity() {
    use TeletextBaseCellCommandKind::{
        CharsetRestore, Glyph, GlyphCharset, LatinCharset, Position,
    };

    let cases = [
        (0, "Ł", None, vec![Position, Glyph]),
        (0, "Ł", Some(vec![]), vec![Position, Glyph]),
        (
            0,
            "é",
            Some(designated_main(0x57)),
            vec![LatinCharset, Glyph, CharsetRestore],
        ),
        (
            0x24,
            "£",
            Some(designated_main(0x37)),
            vec![GlyphCharset, Glyph, CharsetRestore],
        ),
    ];
    for (primary, text, main, expected_kinds) in cases {
        let original = original(primary, text, main);
        let expected = original.clone();
        let encoded = encode_teletext_authored_service(original, &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &expected);
        let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
            panic!("Base compatibility needs a normalized compiled display");
        };
        let owner = expected.semantic().base().pages[0].address;
        let source = page.normalized().original().original_program_source();
        let main = expected.semantic().page_enhancements()[&owner]
            .local_program
            .as_ref();
        assert_eq!(
            *source,
            if main.is_some() {
                TeletextEffectiveProgramSource::Local
            } else {
                TeletextEffectiveProgramSource::BaseOnly
            }
        );
        let mut kinds = Vec::new();
        for command in page.normalized().encoded().command_positions() {
            match command.original_main_source().unwrap() {
                TeletextMainCompositionSource::BaseCell { position, kind } => {
                    assert_eq!(position, TeletextEnhancementPosition::new(1, 8).unwrap());
                    assert_eq!(command.original_local_command(), None);
                    assert_eq!(
                        command.original_base_glyph(),
                        (kind == Glyph).then_some(position)
                    );
                    kinds.push(kind);
                }
                TeletextMainCompositionSource::OriginalPage { command: ordinal } => {
                    assert!(main.unwrap().commands().get(ordinal).is_some());
                    assert_eq!(command.original_local_command(), Some(ordinal));
                    assert_eq!(command.original_base_glyph(), None);
                }
                other => panic!("unexpected generated authority: {other:?}"),
            }
        }
        assert_eq!(kinds, expected_kinds, "primary={primary:02x}, text={text}");
    }
}
