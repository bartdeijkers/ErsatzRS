//! Saved concept reconstruction, not publication or warm receiver proof.
use super::*;
use ersatzrs_app::teletext_design_documents::{
    append_teletext_design_secondary_default_agreements,
    append_teletext_effective_design_secondary_default_agreements,
    TeletextSavedSecondaryDefaultAgreement,
};

fn inspect(
    producers: &[TeletextProducerPresentation],
    encoded: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    agreement: TeletextSecondaryDefaultAgreement,
) {
    assert_eq!(producers.len(), 1);
    assert_eq!(producers[0].secondary_default_agreement(), Some(agreement));
    assert_eq!(
        encoded.declared().semantic().base(),
        producers[0].original().semantic().base()
    );
    for page in producers[0].pages() {
        assert_eq!(
            page.secondary_selection(),
            TeletextProducerSecondarySelection::DecoderDependent
        );
        let enhanced = &encoded.declared().semantic().page_enhancements()[&page.address()];
        let mut expected = *page.effective_defaults();
        expected.secondary = agreement.secondary();
        assert_eq!(enhanced.presentation, Some(expected));
    }
}

#[test]
fn saved_secondary_default_intent_reconstructs_in_both_roles_without_changing_originals() {
    for effective in [false, true] {
        for secondary in [None, Some(1), Some(0x10)] {
            let original = stored_original(effective);
            let choice = secondary.map_or(TeletextSecondaryDefaultChoice::NoSecondG0 {}, |value| {
                TeletextSecondaryDefaultChoice::Designated {
                    designation: TeletextPageCharsetDesignation::new(value).unwrap(),
                }
            });
            let agreement = TeletextSecondaryDefaultAgreement::new(choice);
            let intent = TeletextSavedSecondaryDefaultAgreement::new(0, 1, 1, agreement);
            let saved = if effective {
                append_teletext_effective_design_secondary_default_agreements(
                    &original,
                    &[intent],
                    storage_budget(),
                )
                .unwrap()
                .into_bytes()
            } else {
                append_teletext_design_secondary_default_agreements(
                    &original,
                    &[intent],
                    storage_budget(),
                )
                .unwrap()
                .into_bytes()
            };
            let stored = stored_revision(effective, saved.clone());
            if effective {
                let parts = decode_teletext_effective_design_documents(&saved, storage_budget())
                    .unwrap()
                    .into_parts();
                let restored = compile_effective_restored_parts(
                    stored,
                    parts,
                    bounds(1).0,
                    deadline(),
                    vec![],
                )
                .unwrap();
                inspect(restored.producers(), restored.encoded().unwrap(), agreement);
            } else {
                let parts = decode_teletext_design_documents(&saved, storage_budget())
                    .unwrap()
                    .into_parts();
                let restored =
                    compile_restored_parts(stored, parts, bounds(1).0, deadline(), vec![]).unwrap();
                inspect(restored.producers(), restored.encoded().unwrap(), agreement);
            }
            // Existing concept edits append to the same envelope, retaining the
            // agreement instead of deriving consent from the edited TTI.
            let edited = append_saved(effective, &saved, &saved_overlay(true, vec![]));
            let restored_intent = if effective {
                decode_teletext_effective_design_documents(&edited, storage_budget())
                    .unwrap()
                    .secondary_default_agreements()
                    .to_vec()
            } else {
                decode_teletext_design_documents(&edited, storage_budget())
                    .unwrap()
                    .secondary_default_agreements()
                    .to_vec()
            };
            assert_eq!(restored_intent, [intent]);
            replay_saved(effective, &edited, vec![], 2).unwrap();
            // The retained root and its default-dependent presentation survive.
            replay_saved(effective, &original, vec![], 1).unwrap();
        }
    }
}
