//! Response-owned cells from an immutable published recipe, without authoring.

use super::{teletext_channel_documents::TeletextChannelDocumentEncodingPolicy, teletext_work};
use chrono::{DateTime, Timelike as _, Utc};
use ersatzrs_app::{
    teletext_design_documents::{decode_teletext_design_documents, TeletextDesignDocumentBudget},
    teletext_elements::{
        fields::{FieldSnapshot, FieldValues},
        refresh::{refresh_published_custom_pages, CustomPageRefreshTriggers},
    },
    ApplicationServices,
};
use ersatzrs_domain::{
    TeletextAcceptedGeneration, TeletextCustomPagesDocument, TeletextCustomPagesWire,
    TeletextCustomRowWire, TeletextElementBindingSource,
};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service_before, EncodedTeletextAuthoredService,
    TeletextServiceResourceTransmission,
};
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("published teletext field refresh is unavailable")]
pub(crate) struct RefreshError;

/// One existing admission deadline and cadence, shared by reads and encoding.
pub(crate) struct RefreshContext<'a> {
    pub(crate) application: &'a ApplicationServices,
    pub(crate) channel: i64,
    pub(crate) policy: TeletextChannelDocumentEncodingPolicy,
    pub(crate) deadline: Instant,
    pub(crate) poll: Duration,
}

/// One response's derived rows; unrelated rows/resources retain their native root.
pub(crate) struct PublishedCustomRefresh {
    document: TeletextCustomPagesWire,
    transmissions: Vec<TeletextServiceResourceTransmission>,
    service: Arc<EncodedTeletextAuthoredService>,
    clock: FieldValues,
    next_projection: Option<DateTime<Utc>>,
    pub(crate) next_wake: Instant,
    pub(crate) revision: u64,
    pub(crate) failed_pages: Vec<u16>,
}

impl PublishedCustomRefresh {
    pub(crate) fn service(&self) -> &Arc<EncodedTeletextAuthoredService> {
        &self.service
    }

    /// Load the exact digest-checked manual revision. Legacy recipes stay frozen.
    pub(crate) async fn load(
        context: RefreshContext<'_>,
        generation: &TeletextAcceptedGeneration,
        root: Arc<EncodedTeletextAuthoredService>,
        budget: TeletextDesignDocumentBudget,
    ) -> Result<Option<Arc<Self>>, RefreshError> {
        let application = context.application;
        let deadline = context.deadline;
        if context.channel != generation.request.channel_id {
            return Err(RefreshError);
        }
        let guard = teletext_work::begin(application, Some(deadline))
            .await
            .map_err(|_| RefreshError)?;
        let stored = before(
            deadline,
            application.teletext_design_revision(
                generation.request.design_id,
                generation.request.revision,
                budget.max_serialized_bytes,
            ),
        )
        .await?
        .map_err(|_| RefreshError)?
        .ok_or(RefreshError)?;
        if stored.design_id != generation.request.design_id
            || stored.revision != generation.request.revision
            || stored.digest != generation.request.digest
        {
            return Err(RefreshError);
        }
        let digest = generation.request.digest.clone();
        let recipe = teletext_work::spawn_blocking(guard.hold_work(), move || {
            check(deadline)?;
            if super::teletext_guide_refresh_native::digest(&stored.document) != digest {
                return Err(RefreshError);
            }
            let parts = decode_teletext_design_documents(&stored.document, budget)
                .map_err(|_| RefreshError)?
                .into_parts();
            let Some(index) = parts.selection.bindings.iter().position(|binding| {
                ersatzrs_domain::is_teletext_custom_pages_producer(&binding.addon_id)
            }) else {
                return Ok(None);
            };
            let mut document: TeletextCustomPagesWire =
                serde_json::from_slice(parts.sources.get(index).ok_or(RefreshError)?.bytes())
                    .map_err(|_| RefreshError)?;
            if !document
                .pages
                .iter()
                .any(|page| !page.dynamic_fields.is_empty())
            {
                return Ok(None);
            }
            TeletextCustomPagesDocument::from_wire(document.clone(), &document.generation)
                .map_err(|_| RefreshError)?;
            // Retain final published rows, including editorial edits outside fields.
            for page in &mut document.pages {
                let overlay = root
                    .declared()
                    .semantic()
                    .physical_base_overlays()
                    .values()
                    .find(|overlay| {
                        overlay.owner().page.get() == page.page
                            && overlay.owner().subcode.get() == page.subcode
                    })
                    .ok_or(RefreshError)?;
                page.rows = overlay
                    .rows()
                    .iter()
                    .map(|row| TeletextCustomRowWire::new(row.row(), row.cells()))
                    .collect();
            }
            let banks = parts
                .editorial_overlay
                .as_ref()
                .map_or(&[][..], |overlay| overlay.object_resource_additions());
            let transmissions = super::teletext_stored_designs::resource_transmissions(
                &parts.resource_transmissions,
                banks,
            );
            check(deadline)?;
            Ok(Some((document, transmissions, root)))
        })
        .await
        .map_err(|_| RefreshError)??;
        let Some((document, transmissions, root)) = recipe else {
            return Ok(None);
        };
        let initial = Self {
            document,
            transmissions,
            service: root,
            clock: FieldValues::default(),
            next_projection: None,
            next_wake: Instant::now(),
            revision: 0,
            failed_pages: Vec::new(),
        };
        initial
            .refresh_held(context, guard.hold_work(), true)
            .await
            .map(Some)
    }

    pub(crate) async fn refresh(
        self: &Arc<Self>,
        context: RefreshContext<'_>,
    ) -> Result<Arc<Self>, RefreshError> {
        if Instant::now() < self.next_wake {
            return Ok(Arc::clone(self));
        }
        let guard = teletext_work::begin(context.application, Some(context.deadline))
            .await
            .map_err(|_| RefreshError)?;
        self.refresh_held(context, guard.hold_work(), false).await
    }

    async fn refresh_held(
        &self,
        context: RefreshContext<'_>,
        hold: ersatzrs_core::locking::TeletextWorkHold,
        initial: bool,
    ) -> Result<Arc<Self>, RefreshError> {
        let RefreshContext {
            application,
            channel,
            policy,
            deadline,
            poll,
        } = context;
        let captured = Utc::now();
        let clock = FieldValues::clock(captured);
        let has_programme = self
            .document
            .pages
            .iter()
            .flat_map(|page| &page.dynamic_fields)
            .any(|field| programme(field.source));
        let projection_due =
            has_programme && (initial || self.next_projection.is_none_or(|at| at <= captured));
        let projection = if projection_due {
            // Data failure blanks its logical pages; timeout cannot renew work.
            before(
                deadline,
                application.teletext_element_field_snapshot(channel, captured),
            )
            .await?
            .ok()
            .flatten()
        } else {
            None
        };
        let mut next_projection = self.next_projection;
        if projection_due {
            let poll = chrono::Duration::from_std(poll).map_err(|_| RefreshError)?;
            let reconciliation = captured.checked_add_signed(poll).ok_or(RefreshError)?;
            next_projection = Some(
                projection
                    .as_ref()
                    .and_then(|snapshot| snapshot.next_programme_transition)
                    .map_or(reconciliation, |at| at.min(reconciliation)),
            );
        }
        let triggers = CustomPageRefreshTriggers {
            programme: projection_due,
            date: initial || clock.date != self.clock.date,
            time: initial || clock.time != self.clock.time,
        };
        let (document, failed_pages) = derive_pages(
            &self.document,
            &self.failed_pages,
            triggers,
            projection.as_ref(),
            &clock,
        )?;
        let changed = document != self.document;
        let revision = if changed || failed_pages != self.failed_pages {
            self.revision.checked_add(1).ok_or(RefreshError)?
        } else {
            self.revision
        };
        let service = if changed {
            let root = Arc::clone(&self.service);
            let rendered = document.clone();
            let transmissions = self.transmissions.clone();
            teletext_work::spawn_blocking(hold, move || {
                render(&root, rendered, &transmissions, &policy, deadline)
            })
            .await
            .map_err(|_| RefreshError)??
        } else {
            Arc::clone(&self.service)
        };
        let finished = Utc::now();
        // Refuse values captured before a boundary crossed during encoding.
        if FieldValues::clock(finished) != clock
            || projection
                .as_ref()
                .and_then(|snapshot| snapshot.next_programme_transition)
                .is_some_and(|at| at <= finished)
        {
            return Err(RefreshError);
        }
        let next = next_wake(captured, next_projection)?;
        check(deadline)?;
        Ok(Arc::new(Self {
            document,
            transmissions: self.transmissions.clone(),
            service,
            clock,
            next_projection,
            next_wake: next,
            revision,
            failed_pages,
        }))
    }
}

fn render(
    root: &EncodedTeletextAuthoredService,
    document: TeletextCustomPagesWire,
    transmissions: &[TeletextServiceResourceTransmission],
    policy: &TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<Arc<EncodedTeletextAuthoredService>, RefreshError> {
    check(deadline)?;
    let correlation = document.generation.clone();
    let (_, replacements) = TeletextCustomPagesDocument::from_wire(document, &correlation)
        .map_err(|_| RefreshError)?
        .into_parts();
    let mut overlays = root.declared().semantic().physical_base_overlays().clone();
    for overlay in replacements {
        overlays.insert(overlay.owner(), overlay);
    }
    let declared = root
        .declared()
        .clone()
        .with_physical_base_overlays(overlays.into_values().collect(), policy.validation.service)
        .map_err(|_| RefreshError)?;
    let service = encode_teletext_authored_service_before(
        declared,
        transmissions,
        &policy.encoding,
        deadline,
    )
    .map_err(|_| RefreshError)?
    .with_resource_lifecycle_variants(deadline)
    .map_err(|_| RefreshError)?;
    check(deadline)?;
    Ok(Arc::new(service))
}

fn programme(source: TeletextElementBindingSource) -> bool {
    source.is_dynamic()
        && !matches!(
            source,
            TeletextElementBindingSource::Time | TeletextElementBindingSource::Date
        )
}

/// A failed mixed page stays entirely blank until programme recovery. Other
/// clock-only pages and every fixed cell continue independently.
fn derive_pages(
    original: &TeletextCustomPagesWire,
    previous_failures: &[u16],
    triggers: CustomPageRefreshTriggers,
    projection: Option<&FieldSnapshot>,
    clock: &FieldValues,
) -> Result<(TeletextCustomPagesWire, Vec<u16>), RefreshError> {
    let numbers: BTreeSet<_> = original.pages.iter().map(|page| page.page).collect();
    let mut document = original.clone();
    let mut failed = Vec::new();
    for number in numbers {
        let mut group = original.clone();
        group.pages.retain(|page| page.page == number);
        let has_programme = group
            .pages
            .iter()
            .flat_map(|page| &page.dynamic_fields)
            .any(|field| programme(field.source));
        let was_failed = previous_failures.contains(&number);
        let failure = has_programme
            && if triggers.programme {
                projection.is_none()
            } else {
                was_failed
            };
        let mut due = triggers;
        if was_failed && triggers.programme && !failure {
            due.date = true;
            due.time = true;
        }
        let mut values =
            projection.map_or_else(|| clock.clone(), |snapshot| snapshot.values.clone());
        values.date.clone_from(&clock.date);
        values.time.clone_from(&clock.time);
        let result = refresh_published_custom_pages(&group, due, (!failure).then_some(&values))
            .map_err(|_| RefreshError)?;
        if failure {
            failed.push(number);
        }
        for page in result.document.pages {
            let target = document
                .pages
                .iter_mut()
                .find(|target| target.page == page.page && target.subcode == page.subcode)
                .ok_or(RefreshError)?;
            *target = page;
        }
    }
    Ok((document, failed))
}

fn next_wake(
    now: DateTime<Utc>,
    projection: Option<DateTime<Utc>>,
) -> Result<Instant, RefreshError> {
    let next_minute = now
        .with_second(0)
        .and_then(|at| at.with_nanosecond(0))
        .and_then(|at| at.checked_add_signed(chrono::Duration::minutes(1)))
        .ok_or(RefreshError)?;
    let next = projection.map_or(next_minute, |at| at.min(next_minute));
    let duration = (next - Utc::now()).to_std().unwrap_or(Duration::ZERO);
    Instant::now().checked_add(duration).ok_or(RefreshError)
}
async fn before<T>(
    deadline: Instant,
    future: impl std::future::Future<Output = T>,
) -> Result<T, RefreshError> {
    tokio::time::timeout_at(deadline.into(), future)
        .await
        .map_err(|_| RefreshError)
}
fn check(deadline: Instant) -> Result<(), RefreshError> {
    if Instant::now() < deadline {
        Ok(())
    } else {
        Err(RefreshError)
    }
}

#[cfg(test)]
#[path = "teletext_custom_refresh_tests.rs"]
mod tests;
