//! Exact ordinary-route cycles with one explicitly selected resource update.

use ersatzrs_core::repositories::{
    TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
    TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::TeletextResourceIdentity;
use ersatzrs_ffmpeg::teletext::{TeletextCarousel, TeletextCarouselBatchKind};
use ersatzrs_web::host::teletext_generations::AcceptedTeletextOutputCandidate;
use std::sync::Arc;

const MAX_UNITS: usize = 512;

pub(super) type Pattern = Vec<Vec<[u8; 46]>>;

fn cycle(carousel: &mut TeletextCarousel) -> Vec<[u8; 46]> {
    let mut units = Vec::new();
    for _ in 0..MAX_UNITS {
        let batch = carousel.next_batch().unwrap();
        assert!(units.len() + batch.data_units().len() <= MAX_UNITS);
        units.extend_from_slice(batch.data_units());
        if batch.kind() == TeletextCarouselBatchKind::Terminator {
            return units;
        }
    }
    panic!("fixed service exceeded its bounded complete cycle");
}

pub(super) fn pattern(
    candidate: &AcceptedTeletextOutputCandidate,
    changed: Option<(TeletextResourceIdentity, usize)>,
) -> Pattern {
    assert!(matches!(
        candidate.generation().artifact_version,
        TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION
            | TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION
    ));
    let service = candidate.service();
    let plan = service.resource_lifecycle_variants().unwrap();
    if let Some((identity, counter)) = changed {
        assert!((1..=7).contains(&counter));
        assert!(plan
            .entries()
            .iter()
            .any(|entry| entry.resource() == identity));
    }
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(service)).unwrap();
    let first = cycle(&mut carousel);
    let repeated = cycle(&mut carousel);
    assert_eq!(first.len(), repeated.len());
    first
        .into_iter()
        .zip(repeated)
        .map(|(first, repeat)| {
            if let Some(entry) = plan
                .entries()
                .iter()
                .find(|entry| service.pages()[entry.physical_page_index()].data_units()[0] == first)
            {
                let counter = changed
                    .filter(|(identity, _)| *identity == entry.resource())
                    .map_or(1, |(_, counter)| counter);
                // Preserve complete checked C4/C8 headers for this exact owner;
                // POP/GPOP, MOT and DRCS never share an assumed update counter.
                vec![
                    entry.headers()[counter * 2 + 1],
                    entry.headers()[counter * 2],
                ]
            } else {
                // All body, pointer, extent, display and closing units stay exact.
                vec![first, repeat]
            }
        })
        .collect()
}

pub(super) fn assert_transition(
    candidate: &AcceptedTeletextOutputCandidate,
    selected: Option<(TeletextResourceIdentity, usize)>,
    prefix: &[(usize, [u8; 46])],
    changed_end: usize,
    repeat: &[[u8; 46]],
) {
    assert!(prefix.len() <= MAX_UNITS);
    assert!(repeat.len() <= MAX_UNITS);
    let plan = candidate.service().resource_lifecycle_variants().unwrap();
    if let Some((identity, counter)) = selected {
        assert!((1..=7).contains(&counter));
        assert!(plan
            .entries()
            .iter()
            .any(|entry| entry.resource() == identity));
    }
    for entry in plan.entries() {
        let selected_counter = selected
            .filter(|(identity, _)| *identity == entry.resource())
            .map(|(_, counter)| counter);
        let counter = selected_counter.unwrap_or(1);
        let unchanged = entry.headers()[counter * 2];
        let changed = entry.headers()[counter * 2 + 1];
        // Acquisition may precede the matched whole cycle; count the complete
        // captured prefix through its closing unit rather than hiding that lead.
        let first_count = prefix
            .iter()
            .filter(|(position, unit)| *position <= changed_end && *unit == changed)
            .count();
        assert_eq!(first_count, usize::from(selected_counter.is_some()));
        assert!(repeat.contains(&unchanged));
        assert!(!repeat.contains(&changed));
    }
}
