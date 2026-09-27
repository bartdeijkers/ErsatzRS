use std::collections::HashMap;

use async_trait::async_trait;
use ersatzrs_domain::{EntityId, MediaItemState};

use crate::search::SearchDocument;

use super::{ArtworkReference, Result};

/// Media category used by the global admin-search projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AdminSearchKind {
    Movie,
    Show,
    Season,
    Episode,
    Artist,
    MusicVideo,
    OtherVideo,
    Song,
    Image,
    RemoteStream,
}

/// Storage-hydrated metadata used to build admin search cards.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminSearchEnrichment {
    pub poster: HashMap<EntityId, ArtworkReference>,
    pub thumbnail: HashMap<EntityId, ArtworkReference>,
    pub season_poster: HashMap<EntityId, ArtworkReference>,
    pub show_poster: HashMap<EntityId, ArtworkReference>,
    pub episode_season_id: HashMap<EntityId, EntityId>,
    pub music_video_artist: HashMap<EntityId, String>,
    pub music_video_artist_id: HashMap<EntityId, EntityId>,
    pub song_meta: HashMap<EntityId, (Option<String>, Option<String>)>,
    pub artist_disambiguation: HashMap<EntityId, String>,
    pub state: HashMap<EntityId, MediaItemState>,
}

/// Complete storage-neutral input for the global admin-search page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AdminSearchSnapshot {
    pub documents: Vec<SearchDocument>,
    pub enrichment: HashMap<AdminSearchKind, AdminSearchEnrichment>,
}

#[async_trait]
pub trait AdminSearchRepository: Send + Sync {
    /// Search every indexed admin media kind.
    async fn search_admin_documents(&self, query: &str) -> Result<Vec<SearchDocument>>;

    /// Hydrate card metadata for one media kind.
    async fn enrich_admin_search(
        &self,
        kind: AdminSearchKind,
        ids: &[EntityId],
    ) -> Result<AdminSearchEnrichment>;
}
