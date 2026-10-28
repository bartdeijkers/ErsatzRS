//! Exact, host-authenticated context transitions, never uploaded permissions.
use super::{
    error, BTreeSet, EncodedTeletextAuthoredService, Result, TeletextEditorialEffectsFailure,
    TeletextPageAddress,
};
use ersatzrs_domain::TeletextLevel25PresentationDefaults;

#[derive(Clone, Copy)]
pub(in crate::host) struct SecondaryDefaultContextChange {
    pub owner: TeletextPageAddress,
    pub before: Option<TeletextLevel25PresentationDefaults>,
    pub after: Option<TeletextLevel25PresentationDefaults>,
}

pub(super) fn check(
    changes: &[SecondaryDefaultContextChange],
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    maximum: usize,
) -> Result<()> {
    if changes.len() > maximum {
        return Err(error(None, None, TeletextEditorialEffectsFailure::Budget));
    }
    let mut seen = BTreeSet::new();
    for change in changes {
        let before = original
            .declared()
            .semantic()
            .page_enhancements()
            .get(&change.owner);
        let after = edited
            .declared()
            .semantic()
            .page_enhancements()
            .get(&change.owner);
        if !seen.insert(change.owner)
            || change.before == change.after
            || before.is_none_or(|page| page.presentation != change.before)
            || after.is_none_or(|page| page.presentation != change.after)
        {
            return Err(error(
                Some(change.owner),
                None,
                TeletextEditorialEffectsFailure::Association,
            ));
        }
    }
    Ok(())
}

/// A host-qualified full context can materialize an inactive panel declaration.
/// This is not permission to add/remove a visual panel: exact context provenance
/// and zero Level2.5 widths are required, alongside the caller's width/pixel checks.
pub(super) fn same_panels(
    before: Option<ersatzrs_domain::TeletextSidePanels>,
    after: Option<ersatzrs_domain::TeletextSidePanels>,
    context: Option<&SecondaryDefaultContextChange>,
) -> bool {
    if before == after {
        return true;
    }
    let Some(context) = context else {
        return false;
    };
    if before != context.before.map(|value| value.panels)
        || after != context.after.map(|value| value.panels)
    {
        return false;
    }
    match (before, after) {
        (None, Some(panels)) | (Some(panels), None) => panels.level25_widths() == (0, 0),
        _ => false,
    }
}

#[derive(Clone, Copy)]
pub(super) struct PacketChanges {
    pub operands: bool,
    pub repacking: bool,
    pub links: bool,
    pub secondary: SecondaryPacketChange,
}

#[derive(Clone, Copy)]
pub(super) enum SecondaryPacketChange {
    Unchanged,
    Authorized,
}
impl PacketChanges {
    pub(super) fn protects(self, record: &super::TeletextEncodedDisplayRecord) -> bool {
        !(matches!(self.secondary, SecondaryPacketChange::Authorized)
            && matches!(
                record,
                super::TeletextEncodedDisplayRecord::Triplets {
                    row: 28,
                    designation: 0,
                    ..
                }
            ))
            && super::protected_packet(record, self.repacking, self.links)
    }
}
