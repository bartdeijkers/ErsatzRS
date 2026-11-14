//! Pair derived field cells with their unchanged, independently confirmed root.

use super::{ReadFailure, SelectedTeletextOutputCandidate, TeletextAcceptedUpdatesContext};
use crate::host::teletext_custom_refresh::{PublishedCustomRefresh, RefreshContext};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use std::{ops::Deref, sync::Arc};

pub(in crate::transport::iptv::teletext_live_output) struct NativeCandidate {
    pub(super) selected: Arc<SelectedTeletextOutputCandidate>,
    pub(super) custom: Option<Arc<PublishedCustomRefresh>>,
}

impl Deref for NativeCandidate {
    type Target = SelectedTeletextOutputCandidate;
    fn deref(&self) -> &Self::Target {
        &self.selected
    }
}

impl NativeCandidate {
    pub(in crate::transport::iptv::teletext_live_output) fn fields_current_at(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> bool {
        self.custom
            .as_ref()
            .is_none_or(|custom| custom.current_at(now))
    }

    pub(in crate::transport::iptv::teletext_live_output) async fn prepare(
        context: &TeletextAcceptedUpdatesContext,
        selected: Arc<SelectedTeletextOutputCandidate>,
        previous: Option<&Self>,
        deadline: tokio::time::Instant,
    ) -> Result<Arc<Self>, ReadFailure> {
        let deadline = selected
            .next_transition()
            .map_or(deadline, |cap| deadline.min(cap));
        let retained = previous.filter(|previous| {
            previous.generation() == selected.generation()
                && previous.guide_identity() == selected.guide_identity()
                && previous
                    .selected
                    .service()
                    .zip(selected.service())
                    .is_some_and(|(before, after)| Arc::ptr_eq(before, after))
        });
        let refresh = RefreshContext {
            application: &context.application,
            channel: context.channel_id,
            policy: context.encoding.clone(),
            deadline: deadline.into_std(),
            poll: context.timing.poll_interval,
        };
        let custom = match retained {
            Some(previous) => match &previous.custom {
                Some(custom) => Some(custom.refresh(refresh).await.map_err(|_| ReadFailure)?),
                None => None,
            },
            None if selected.supports_custom_field_refresh() => match selected.service() {
                Some(root) => PublishedCustomRefresh::load(
                    refresh,
                    selected.generation(),
                    Arc::clone(root),
                    context.budget,
                )
                .await
                .map_err(|_| ReadFailure)?,
                None => None,
            },
            None => None,
        };
        Ok(Arc::new(Self { selected, custom }))
    }

    pub(in crate::transport::iptv::teletext_live_output) fn service(
        &self,
    ) -> Option<&Arc<EncodedTeletextAuthoredService>> {
        self.custom
            .as_ref()
            .map(|custom| custom.service())
            .or_else(|| self.selected.service())
    }
}
