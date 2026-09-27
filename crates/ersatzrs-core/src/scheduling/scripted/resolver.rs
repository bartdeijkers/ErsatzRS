use super::ScriptState;
use crate::repositories::Result;
use crate::scheduling::{
    SchedulingContentResolver, SchedulingContentSource, SchedulingMediaItem, SchedulingPlaylistItem,
};
use ersatzrs_domain::EntityId;

/// Fallible repository access from the synchronous Lua scheduling worker.
/// Implementations keep storage and async-runtime details outside the core.
pub trait ScriptedContentResolver: std::fmt::Debug + Send + Sync {
    /// Resolve a media source, preserving repository errors.
    ///
    /// # Errors
    /// Returns a source lookup or decode failure.
    fn media_items(&self, source: &SchedulingContentSource) -> Result<Vec<SchedulingMediaItem>>;
    /// Resolve structured playlist children.
    ///
    /// # Errors
    /// Returns a playlist lookup or decode failure.
    fn playlist_items(
        &self,
        source: &SchedulingContentSource,
    ) -> Result<Option<Vec<SchedulingPlaylistItem>>>;
    /// Resolve a named watermark.
    ///
    /// # Errors
    /// Returns a repository lookup failure.
    fn watermark_id(&self, name: &str) -> Result<Option<EntityId>>;
    /// Resolve a graphics element path.
    ///
    /// # Errors
    /// Returns a repository lookup failure.
    fn graphics_id(&self, path: &str) -> Result<Option<EntityId>>;
}

impl ScriptState {
    pub(super) fn source_media(
        &mut self,
        source: &SchedulingContentSource,
    ) -> mlua::Result<Vec<SchedulingMediaItem>> {
        let items = self.resolver.as_ref().map_or_else(
            || Ok(self.reference_data.media_items_for_source(source)),
            |resolver| resolver.media_items(source).map_err(mlua::Error::external),
        )?;
        self.validate_source_media(items)
    }

    pub(super) fn source_playlist(
        &mut self,
        source: &SchedulingContentSource,
    ) -> mlua::Result<Option<Vec<SchedulingPlaylistItem>>> {
        let mut children = self.resolver.as_ref().map_or_else(
            || Ok(self.reference_data.playlist_items_for_source(source)),
            |resolver| {
                resolver
                    .playlist_items(source)
                    .map_err(mlua::Error::external)
            },
        )?;
        if let Some(children) = children.as_mut() {
            for child in children {
                child.media_items =
                    self.validate_source_media(std::mem::take(&mut child.media_items))?;
            }
        }
        Ok(children)
    }

    fn validate_source_media(
        &mut self,
        mut items: Vec<SchedulingMediaItem>,
    ) -> mlua::Result<Vec<SchedulingMediaItem>> {
        // Runtime lookups happen after the builder's reference-data validation.
        // Apply the same duration rules and non-Classic live-source restriction.
        items.retain(|item| {
            if crate::playout_builder::unschedulable_remote_stream(item) {
                let warning = format!(
                    "Skipped Remote Stream {} with unknown or zero duration",
                    item.id
                );
                if !self.warnings.contains(&warning) {
                    self.warnings.push(warning);
                }
                false
            } else {
                true
            }
        });
        if let Some(item) = items
            .iter()
            .find(|item| item.liveness == Some(ersatzrs_domain::RemoteStreamLiveness::Live))
        {
            return Err(mlua::Error::external(
                super::SchedulingError::UnsupportedLiveSourcePlacement(item.id),
            ));
        }
        Ok(items)
    }

    pub(super) fn watermark_id(&self, name: &str) -> mlua::Result<Option<EntityId>> {
        self.resolver.as_ref().map_or_else(
            || Ok(self.reference_data.watermark_ids_by_name.get(name).copied()),
            |resolver| resolver.watermark_id(name).map_err(mlua::Error::external),
        )
    }

    pub(super) fn graphics_id(&self, path: &str) -> mlua::Result<Option<EntityId>> {
        self.resolver.as_ref().map_or_else(
            || Ok(self.reference_data.graphics_ids_by_path.get(path).copied()),
            |resolver| resolver.graphics_id(path).map_err(mlua::Error::external),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduling::{
        SchedulingCollectionKey, SchedulingReferenceData, SchedulingSmartShuffleState,
    };
    use chrono::{Duration, TimeZone, Utc};
    use ersatzrs_domain::{PlaybackOrder, RemoteStreamLiveness};
    use std::sync::Arc;

    #[derive(Debug)]
    struct Sources(Vec<SchedulingMediaItem>);

    impl ScriptedContentResolver for Sources {
        fn media_items(&self, _: &SchedulingContentSource) -> Result<Vec<SchedulingMediaItem>> {
            Ok(self.0.clone())
        }

        fn playlist_items(
            &self,
            _: &SchedulingContentSource,
        ) -> Result<Option<Vec<SchedulingPlaylistItem>>> {
            Ok(Some(vec![SchedulingPlaylistItem {
                collection_key: SchedulingCollectionKey::default(),
                playback_order: PlaybackOrder::None,
                play_all: true,
                count: None,
                media_items: self.0.clone(),
            }]))
        }

        fn watermark_id(&self, _: &str) -> Result<Option<EntityId>> {
            Ok(None)
        }
        fn graphics_id(&self, _: &str) -> Result<Option<EntityId>> {
            Ok(None)
        }
    }

    #[test]
    fn scripted_runtime_sources_preserve_remote_stream_safety() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut state = ScriptState::new(
            1,
            start,
            start + Duration::hours(1),
            crate::scheduling::PlayoutBuildMode::Reset,
            SchedulingReferenceData::default(),
            SchedulingSmartShuffleState::default(),
        );
        let mut unknown = SchedulingMediaItem::new(2, Duration::minutes(1));
        unknown.liveness = Some(RemoteStreamLiveness::Unknown);
        let mut zero = SchedulingMediaItem::new(3, Duration::zero());
        zero.liveness = Some(RemoteStreamLiveness::Finite);
        state.resolver = Some(Arc::new(Sources(vec![
            SchedulingMediaItem::new(1, Duration::minutes(1)),
            unknown,
            zero,
        ])));
        let source = SchedulingContentSource::Collection("dynamic".into());
        let media = state.source_media(&source).unwrap();
        assert_eq!(media.iter().map(|item| item.id).collect::<Vec<_>>(), [1]);
        let children = state.source_playlist(&source).unwrap().unwrap();
        assert_eq!(children[0].media_items, media);
        assert_eq!(state.warnings.len(), 2);

        let mut live = SchedulingMediaItem::new(4, Duration::minutes(1));
        live.liveness = Some(RemoteStreamLiveness::Live);
        state.resolver = Some(Arc::new(Sources(vec![live])));
        assert!(state.source_media(&source).is_err());
        assert!(state.source_playlist(&source).is_err());
    }
}
