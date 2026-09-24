//! Demand-time filler policy from ErsatzTV's `DynamicPlayoutItemService`.
//! Reads current policy and content without changing persisted scheduling state.

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::{
    repositories::{ConfigRepository, DecoRepository, Result},
    scheduling::{SchedulingCollectionKey, SchedulingMediaItem},
};
use ersatzrs_domain::{Channel, DecoMode, FillerKind, PlayoutItem};
use sqlx::Row;

use crate::{
    channel_playout::ChannelPlayoutContext,
    db::DbPool,
    playback::active_template_deco_id,
    repositories::{
        common::{decode_datetime, decode_enum_column, encode_datetime},
        make_repositories, SqliteSchedulingContentRepository, SqlxResultExt as _,
    },
};

/// Current candidates and target-channel bounds for one unscheduled interval.
/// Selection is kept separate so a session can coalesce repeated callbacks.
pub struct FallbackCandidates {
    pub items: Vec<SchedulingMediaItem>,
    pub allow_watermarks: bool,
    pub playout_id: i64,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
}

impl FallbackCandidates {
    /// Materialize a chosen candidate without inserting a `PlayoutItem` row.
    /// Zero-length media cannot fill a gap. Exact playlist track pins survive.
    #[must_use]
    pub fn materialize(&self, index: usize) -> Option<PlayoutItem> {
        let media = self.items.get(index)?;
        let duration = media.duration.min(self.finish - self.start);
        if duration <= Duration::zero() {
            return None;
        }
        let in_point = media.chapter.as_ref().map_or(Duration::zero(), |c| c.start);
        Some(PlayoutItem {
            media_item_id: media.id,
            start: self.start,
            finish: self.start.checked_add_signed(duration)?,
            playout_id: self.playout_id,
            filler_kind: FillerKind::Fallback,
            in_point,
            out_point: in_point.checked_add(&duration)?,
            disable_watermarks: !self.allow_watermarks,
            preferred_audio_language_code: media.preferred_audio_language_code.clone(),
            preferred_audio_title: media.preferred_audio_title.clone(),
            preferred_subtitle_language_code: media.preferred_subtitle_language_code.clone(),
            subtitle_mode: media.subtitle_mode,
            pinned_stream_selection: media.pinned_stream_selection,
            ..PlayoutItem::default()
        })
    }
}

/// Resolve decoration/channel/global fallback precedence in a current gap.
/// `now` and `until` are target-channel times; mirror conversion happens once.
/// A disabled or empty selected collection does not cascade to another preset.
/// Callers own admission, build locks, random choice and subsequent validation.
///
/// # Errors
/// Returns current database/content lookup or timestamp conversion failures.
pub async fn load_candidates(
    pool: &DbPool,
    channel: &Channel,
    now: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<Option<FallbackCandidates>> {
    if until <= now {
        return Ok(None);
    }
    let context = ChannelPlayoutContext::for_channel(channel);
    let source_now = context.source_time(now)?;
    let occupied: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "PlayoutItem" pi
           JOIN "Playout" p ON p."Id"=pi."PlayoutId"
           WHERE p."ChannelId"=? AND pi."Start"<=? AND pi."Finish">?)"#,
    )
    .bind(context.source_channel_id)
    .bind(encode_datetime(source_now))
    .bind(encode_datetime(source_now))
    .fetch_one(pool)
    .await
    .repository()?;
    if occupied {
        return Ok(None);
    }
    let next: Option<String> = sqlx::query_scalar(
        r#"SELECT MIN(pi."Start") FROM "PlayoutItem" pi
           JOIN "Playout" p ON p."Id"=pi."PlayoutId"
           WHERE p."ChannelId"=? AND pi."Start">?"#,
    )
    .bind(context.source_channel_id)
    .bind(encode_datetime(source_now))
    .fetch_one(pool)
    .await
    .repository()?;
    let finish = match next {
        Some(next) => {
            until.min(context.target_time(decode_datetime("fallback next start", &next)?)?)
        }
        None => until,
    };
    let playout = sqlx::query(
        r#"SELECT "Id", "DecoId" FROM "Playout" WHERE "ChannelId"=? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(context.source_channel_id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let mut deco_ids = Vec::new();
    let mut playout_id = 0;
    if let Some(playout) = playout {
        playout_id = playout.try_get("Id").repository()?;
        deco_ids.extend(active_template_deco_id(pool, playout_id, source_now).await?);
        deco_ids.extend(playout.try_get::<Option<i64>, _>("DecoId").repository()?);
    }
    let Some((key, allow_watermarks)) = policy(pool, channel, &deco_ids).await? else {
        return Ok(None);
    };
    let items = SqliteSchedulingContentRepository::new(pool.clone())
        .items_for_collection_key(&key, 0)
        .await?;
    if items.is_empty() {
        return Ok(None);
    }
    Ok(Some(FallbackCandidates {
        items,
        allow_watermarks,
        playout_id,
        start: now,
        finish,
    }))
}

async fn policy(
    pool: &DbPool,
    channel: &Channel,
    deco_ids: &[i64],
) -> Result<Option<(SchedulingCollectionKey, bool)>> {
    let repos = make_repositories(pool.clone());
    for id in deco_ids {
        let Some(deco) = repos.deco.get_admin(*id).await? else {
            continue;
        };
        match deco.dead_air_fallback_mode {
            DecoMode::Disable => return Ok(None),
            DecoMode::Override => {
                return Ok(Some((
                    SchedulingCollectionKey {
                        collection_type: deco.dead_air_fallback_collection_type,
                        collection_id: deco.dead_air_fallback_collection_id,
                        media_item_id: deco.dead_air_fallback_media_item_id,
                        multi_collection_id: deco.dead_air_fallback_multi_collection_id,
                        smart_collection_id: deco.dead_air_fallback_smart_collection_id,
                        ..Default::default()
                    },
                    true,
                )))
            }
            DecoMode::Inherit | DecoMode::Merge => {}
        }
    }
    if let Some(id) = channel.fallback_filler_id {
        if let Some(preset) = preset(pool, id).await? {
            return Ok(Some(preset));
        }
    }
    let global = repos
        .config
        .get(ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID)
        .await?;
    match global.and_then(|value| value.parse::<i64>().ok()) {
        Some(id) => preset(pool, id).await,
        None => Ok(None),
    }
}

async fn preset(pool: &DbPool, id: i64) -> Result<Option<(SchedulingCollectionKey, bool)>> {
    let Some(row) = sqlx::query(r#"SELECT * FROM "FillerPreset" WHERE "Id"=?"#)
        .bind(id)
        .fetch_optional(pool)
        .await
        .repository()?
    else {
        return Ok(None);
    };
    Ok(Some((
        SchedulingCollectionKey {
            collection_type: decode_enum_column(
                "fallback collection type",
                &row,
                "CollectionType",
            )?,
            collection_id: row.try_get("CollectionId").repository()?,
            media_item_id: row.try_get("MediaItemId").repository()?,
            multi_collection_id: row.try_get("MultiCollectionId").repository()?,
            smart_collection_id: row.try_get("SmartCollectionId").repository()?,
            playlist_id: row.try_get("PlaylistId").repository()?,
            ..Default::default()
        },
        row.try_get("AllowWatermarks").repository()?,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::repositories::PlayoutRepository as _;
    use ersatzrs_domain::{ChannelPlayoutSource, PinnedStreamSelection};

    async fn fixture() -> (DbPool, Channel, DateTime<Utc>) {
        let (pool, _, mut channel, start) = crate::next_engine::schedule::tests::fixture().await;
        make_repositories(pool.clone())
            .playout
            .upsert_item(&PlayoutItem {
                id: 2,
                playout_id: 1,
                media_item_id: 10,
                start: start + Duration::seconds(180),
                finish: start + Duration::seconds(240),
                ..Default::default()
            })
            .await
            .unwrap();
        for sql in [
            r#"INSERT INTO "FillerPreset" ("Id","AllowWatermarks","CollectionType","MediaItemId","FillerKind","FillerMode","UseChaptersAsMediaItems") VALUES (1,0,10,10,4,0,0),(2,1,10,10,4,0,0)"#,
            r#"INSERT INTO "DecoGroup" ("Id","Name") VALUES (1,'Fixture')"#,
            r#"INSERT INTO "Deco" ("Id","Name","DecoGroupId","BreakContentMode","DeadAirFallbackCollectionType","DeadAirFallbackMediaItemId","DeadAirFallbackMode","DefaultFillerCollectionType","DefaultFillerMode","DefaultFillerTrimToFit","GraphicsElementsMode","UseGraphicsElementsDuringFiller","UseWatermarkDuringFiller","WatermarkMode") VALUES (1,'Disable',1,0,10,10,1,0,0,0,0,0,0,0),(2,'Override',1,0,10,10,2,0,0,0,0,0,0,0)"#,
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        channel.fallback_filler_id = Some(1);
        make_repositories(pool.clone())
            .config
            .set(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID,
                "2",
            )
            .await
            .unwrap();
        (pool, channel, start)
    }

    #[tokio::test]
    async fn current_gap_clips_filler_and_preserves_persisted_programmes() {
        let (pool, channel, start) = fixture().await;
        let repos = make_repositories(pool.clone());
        let before = repos
            .playout
            .items_in_range(1, start, start + Duration::hours(1))
            .await
            .unwrap();
        for (seconds, duration) in [(90, 60), (150, 30)] {
            let now = start + Duration::seconds(seconds);
            let selected = load_candidates(&pool, &channel, now, start + Duration::hours(1))
                .await
                .unwrap()
                .unwrap()
                .materialize(0)
                .unwrap();
            assert_eq!(selected.start, now);
            assert_eq!(selected.finish, now + Duration::seconds(duration));
            assert_eq!(selected.out_point, Duration::seconds(duration));
            assert_eq!(selected.filler_kind, FillerKind::Fallback);
            assert!(selected.disable_watermarks);
            assert_eq!(selected.id, 0);
        }
        assert!(
            load_candidates(&pool, &channel, start, start + Duration::hours(1))
                .await
                .unwrap()
                .is_none()
        );
        assert!(load_candidates(&pool, &channel, start, start)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            before,
            repos
                .playout
                .items_in_range(1, start, start + Duration::hours(1))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn decoration_disable_override_channel_and_global_precedence() {
        let (pool, mut channel, _) = fixture().await;
        assert!(policy(&pool, &channel, &[1, 2]).await.unwrap().is_none());
        assert!(policy(&pool, &channel, &[2, 1]).await.unwrap().unwrap().1);
        assert!(!policy(&pool, &channel, &[]).await.unwrap().unwrap().1);
        channel.fallback_filler_id = None;
        assert!(policy(&pool, &channel, &[]).await.unwrap().unwrap().1);
        channel.fallback_filler_id = Some(999);
        assert!(policy(&pool, &channel, &[]).await.unwrap().unwrap().1);
    }

    #[tokio::test]
    async fn empty_selected_collection_does_not_fall_back_to_global() {
        let (pool, channel, start) = fixture().await;
        sqlx::query(
            r#"UPDATE "FillerPreset" SET "CollectionType"=0,"MediaItemId"=NULL WHERE "Id"=1"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(load_candidates(
            &pool,
            &channel,
            start + Duration::seconds(90),
            start + Duration::hours(1)
        )
        .await
        .unwrap()
        .is_none());
    }

    #[tokio::test]
    async fn active_template_uses_source_time_and_exclusive_end_before_playout_deco() {
        let (pool, mut channel, start) = fixture().await;
        for sql in [
            r#"INSERT INTO "TemplateGroup" ("Id") VALUES (1)"#,
            r#"INSERT INTO "Template" ("Id","DateUpdated","TemplateGroupId") VALUES (1,'2026-09-24',1)"#,
            r#"INSERT INTO "DecoTemplateGroup" ("Id") VALUES (1)"#,
            r#"INSERT INTO "DecoTemplate" ("Id","DateUpdated","DecoTemplateGroupId") VALUES (1,'2026-09-24',1)"#,
            r#"INSERT INTO "DecoTemplateItem" ("Id","DecoTemplateId","DecoId","StartTime","EndTime") VALUES (1,1,1,'12:01:00','12:02:00')"#,
            r#"INSERT INTO "PlayoutTemplate" ("Id","DateUpdated","DecoTemplateId","EndDay","EndMonth","Index","LimitToDateRange","PlayoutId","StartDay","StartMonth","TemplateId") VALUES (1,'2026-09-24',1,31,12,0,0,1,1,1,1)"#,
            r#"UPDATE "Playout" SET "DecoId"=2 WHERE "Id"=1"#,
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        channel.id = 2;
        channel.playout_source = ChannelPlayoutSource::Mirror;
        channel.mirror_source_channel_id = Some(1);
        channel.playout_offset = Some(Duration::hours(1));
        let now = start + Duration::hours(1) + Duration::seconds(90);
        assert!(
            load_candidates(&pool, &channel, now, now + Duration::minutes(5))
                .await
                .unwrap()
                .is_none()
        );
        let at_end = now + Duration::seconds(30);
        assert!(
            load_candidates(&pool, &channel, at_end, at_end + Duration::minutes(5))
                .await
                .unwrap()
                .unwrap()
                .allow_watermarks
        );
    }

    #[tokio::test]
    async fn mirror_bounds_and_explicit_track_pins_survive_materialization() {
        let (pool, mut channel, start) = fixture().await;
        channel.id = 2;
        channel.playout_source = ChannelPlayoutSource::Mirror;
        channel.mirror_source_channel_id = Some(1);
        channel.playout_offset = Some(Duration::hours(1));
        let now = start + Duration::hours(1) + Duration::seconds(150);
        let mut candidates = load_candidates(&pool, &channel, now, now + Duration::hours(1))
            .await
            .unwrap()
            .unwrap();
        let pin = PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: None,
            subtitle_stream_index: None,
        };
        candidates.items[0].pinned_stream_selection = Some(pin);
        let item = candidates.materialize(0).unwrap();
        assert_eq!(
            item.finish,
            start + Duration::hours(1) + Duration::seconds(180)
        );
        assert_eq!(item.pinned_stream_selection, Some(pin));
        assert_eq!(item.playout_id, 1);
        candidates.items[0].duration = Duration::zero();
        assert!(candidates.materialize(0).is_none());
    }
}
