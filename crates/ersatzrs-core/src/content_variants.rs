//! Operator examples for ADR 0018 content-variant scheduling.

use ersatzrs_domain::{
    CollectionType, FillerKind, FillerMode, FillerPreset, GuideMode, PlaybackOrder, PlayoutMode,
    ProgramScheduleItem,
};

/// A smart-collection query an operator can save as the first step of an
/// ADR 0018 content-variant workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentVariantSmartCollectionExample {
    pub slug: &'static str,
    pub name: &'static str,
    pub query: &'static str,
}

pub const UNPLUGGED_RENDITIONS: ContentVariantSmartCollectionExample =
    ContentVariantSmartCollectionExample {
        slug: "unplugged-renditions",
        name: "Example - Unplugged Renditions",
        query: "content_kind:alternate_version AND sub_kind:unplugged",
    };

pub const COMMENTARY_MOVIES: ContentVariantSmartCollectionExample =
    ContentVariantSmartCollectionExample {
        slug: "commentary-movies",
        name: "Example - Commentary Movies",
        query: "type:movie AND has_commentary:true AND audio_title:commentary",
    };

pub const FEATURETTE_PRE_ROLL: ContentVariantSmartCollectionExample =
    ContentVariantSmartCollectionExample {
        slug: "featurette-pre-roll",
        name: "Example - Featurette Pre-Roll",
        query: "content_kind:extra AND sub_kind:featurette",
    };

pub const MOVIE_PRIMARIES: ContentVariantSmartCollectionExample =
    ContentVariantSmartCollectionExample {
        slug: "movie-primaries",
        name: "Example - Movie Primaries",
        query: "type:movie",
    };

pub const SMART_COLLECTION_EXAMPLES: &[ContentVariantSmartCollectionExample] = &[
    UNPLUGGED_RENDITIONS,
    COMMENTARY_MOVIES,
    FEATURETTE_PRE_ROLL,
    MOVIE_PRIMARIES,
];

/// Returns the built-in ADR 0018 smart-collection examples.
#[must_use]
pub const fn smart_collection_examples() -> &'static [ContentVariantSmartCollectionExample] {
    SMART_COLLECTION_EXAMPLES
}

/// Builds the featurette pre-roll filler preset for the saved
/// `FEATURETTE_PRE_ROLL` smart collection.
#[must_use]
pub fn featurette_pre_roll_filler(id: i64, featurette_smart_collection_id: i64) -> FillerPreset {
    FillerPreset {
        id,
        name: "Example - Featurette Pre-Roll Filler".to_owned(),
        filler_kind: FillerKind::PreRoll,
        filler_mode: FillerMode::Count,
        collection_type: CollectionType::SmartCollection,
        smart_collection_id: Some(featurette_smart_collection_id),
        count: Some(1),
        allow_watermarks: false,
        playback_order: PlaybackOrder::Chronological,
        ..FillerPreset::default()
    }
}

/// Builds the movie schedule item that consumes the featurette pre-roll filler.
#[must_use]
pub fn movie_schedule_item_with_featurette_pre_roll(
    id: i64,
    program_schedule_id: i64,
    movie_smart_collection_id: i64,
    pre_roll_filler_id: i64,
) -> ProgramScheduleItem {
    ProgramScheduleItem {
        id,
        program_schedule_id,
        collection_type: CollectionType::SmartCollection,
        smart_collection_id: Some(movie_smart_collection_id),
        pre_roll_filler_id: Some(pre_roll_filler_id),
        playout_mode: PlayoutMode::One,
        playback_order: PlaybackOrder::Chronological,
        guide_mode: GuideMode::Normal,
        sort_order: 1,
        ..ProgramScheduleItem::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn examples_use_variant_tokens_that_opt_into_hidden_rows() {
        let queries = smart_collection_examples()
            .iter()
            .map(|example| example.query)
            .collect::<Vec<_>>();

        assert!(queries.contains(&"content_kind:alternate_version AND sub_kind:unplugged"));
        assert!(queries.contains(&"type:movie AND has_commentary:true AND audio_title:commentary"));
        assert!(queries.contains(&"content_kind:extra AND sub_kind:featurette"));
        assert!(queries.contains(&"type:movie"));
    }

    #[test]
    fn featurette_pre_roll_templates_do_not_attach_to_a_schedule_by_default() {
        let filler = featurette_pre_roll_filler(10, 20);
        assert_eq!(filler.filler_kind, FillerKind::PreRoll);
        assert_eq!(filler.filler_mode, FillerMode::Count);
        assert_eq!(filler.collection_type, CollectionType::SmartCollection);
        assert_eq!(filler.smart_collection_id, Some(20));
        assert_eq!(filler.count, Some(1));
        assert!(!filler.allow_watermarks);

        let item = movie_schedule_item_with_featurette_pre_roll(30, 40, 50, 10);
        assert_eq!(item.program_schedule_id, 40);
        assert_eq!(item.collection_type, CollectionType::SmartCollection);
        assert_eq!(item.smart_collection_id, Some(50));
        assert_eq!(item.pre_roll_filler_id, Some(10));
        assert_eq!(item.playout_mode, PlayoutMode::One);
    }
}
