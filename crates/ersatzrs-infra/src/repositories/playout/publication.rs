//! Prepare one complete scheduling transaction before making it visible.

use super::*;

/// An unpublished build, including its timeline, history and continuation state.
///
/// Other connections keep reading the accepted build until `commit`. Dropping
/// this owner queues `SQLite` rollback, including cancellation of its caller.
/// The owning maintenance operation must retain its playout lock and bound any
/// preparation work; this transaction holds `SQLite`'s writer reservation.
#[must_use = "commit the prepared build or drop it to roll back all changes"]
pub struct PendingPlayoutBuild {
    tx: Transaction<'static, crate::db::Db>,
    playout_id: EntityId,
    channel_id: EntityId,
    deadline: Option<std::time::Instant>,
}

impl PendingPlayoutBuild {
    pub(crate) async fn overlay_context(
        &mut self,
        item_id: EntityId,
        source_time: DateTime<Utc>,
    ) -> Result<crate::playback::CandidateOverlayContext> {
        validate_deadline(self.deadline)?;
        crate::playback::CandidateOverlayContext::read(&mut self.tx, item_id, source_time).await
    }

    pub(crate) const fn playout_id(&self) -> EntityId {
        self.playout_id
    }

    pub(crate) const fn channel_id(&self) -> EntityId {
        self.channel_id
    }

    pub(crate) const fn publication_deadline(&self) -> Option<std::time::Instant> {
        self.deadline
    }

    pub(crate) fn restrict_deadline(&mut self, deadline: std::time::Instant) {
        self.deadline = Some(
            self.deadline
                .map_or(deadline, |previous| previous.min(deadline)),
        );
    }

    /// Read the candidate with its real database identities and full attachments.
    /// This uses the same decoder as committed playback/guide reads.
    ///
    /// # Errors
    /// Returns database, decode or expired-publication errors.
    pub async fn items_in_range(
        &mut self,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
    ) -> Result<Vec<PlayoutItem>> {
        validate_deadline(self.deadline)?;
        read_items(&mut self.tx, self.playout_id, start, finish).await
    }

    /// Make all prepared scheduling state visible together.
    ///
    /// # Errors
    /// Returns a database error or refuses an expired programme boundary.
    pub async fn commit(self) -> Result<()> {
        validate_deadline(self.deadline)?;
        self.tx.commit().await.repository()
    }
}

fn validate_deadline(deadline: Option<std::time::Instant>) -> Result<()> {
    if deadline.is_some_and(|limit| std::time::Instant::now() >= limit) {
        return Err(RepositoryError::unavailable(
            SqlitePlayoutRepository::PRESERVATION_PUBLICATION_OPERATION,
            "The publication deadline for the retained programme boundary elapsed. The published timeline was not changed; retry using the current programme.",
        ));
    }
    Ok(())
}

impl SqlitePlayoutRepository {
    /// Prepare a build without exposing its rows to other connections.
    ///
    /// Reuses the ordinary source fences and transactional scheduler writes.
    /// The caller must keep its playout lock until publication is settled and
    /// must not perform unbounded external work while holding this writer.
    ///
    /// # Errors
    /// Returns the same validation, source-fence and write errors as applying a
    /// build directly. Failure or cancellation leaves committed state intact.
    #[allow(clippy::too_many_lines)] // Keep the existing atomic scheduler write set in one transaction.
    pub async fn prepare_build_result(
        &self,
        result: &SchedulingResult,
        playout: &Playout,
        max_playout_offset: chrono::Duration,
    ) -> Result<PendingPlayoutBuild> {
        validate_deadline(result.publication_deadline)?;
        let source_fence = if playout.schedule_kind
            == ersatzrs_domain::PlayoutScheduleKind::Scripted
            && !result.scripted_sources.is_empty()
        {
            Some(
                crate::scripted_dependencies::PublicationFence::prepare(
                    &self.0,
                    &result.scripted_sources,
                )
                .await?,
            )
        } else {
            None
        };
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;

        validate_deadline(result.publication_deadline)?;
        if let Some(fence) = source_fence {
            fence.validate(&mut tx).await?;
        }

        // ── items ─────────────────────────────────────────────────────────────
        if let Some(guard) = &result.publication_guard {
            if !crate::scan_refresh::publication_guard_matches(&mut tx, playout.id, guard)
                .await
                .repository()?
            {
                return Err(RepositoryError::Conflict(
                    "Playout build superseded by a changed source revision; the published timeline was not changed. Retry the refresh using current sources".to_owned(),
                ));
            }
        }
        if playout.schedule_kind == ersatzrs_domain::PlayoutScheduleKind::Scripted {
            scripted_timeline::validate_publication(
                &mut tx,
                playout.id,
                result,
                max_playout_offset,
            )
            .await?;
        }
        if playout.schedule_kind == ersatzrs_domain::PlayoutScheduleKind::Classic {
            classic_timeline::validate_publication(&mut tx, playout.id, result, max_playout_offset)
                .await?;
        }
        if result.clear_items {
            if let Some(seam) = result.retain_through {
                let cutoff = result.remove_before.unwrap_or(seam)
                    - max_playout_offset.max(chrono::Duration::zero());
                sqlx::query(
                    r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ?
                       AND ("Finish" > ? OR "Finish" < ?)"#,
                )
                .bind(playout.id)
                .bind(encode_datetime(seam))
                .bind(encode_datetime(cutoff))
                .execute(&mut *tx)
                .await
                .repository()?;
            } else {
                sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ?"#)
                    .bind(playout.id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
        } else {
            for &item_id in &result.items_to_remove {
                sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "Id" = ?"#)
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            if let Some(cutoff) = result.remove_before {
                let cutoff = cutoff - max_playout_offset.max(chrono::Duration::zero());
                sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ? AND "Finish" < ?"#)
                    .bind(playout.id)
                    .bind(encode_datetime(cutoff))
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
        }

        for item in &result.added_items {
            // Insert without an explicit Id so SQLite auto-assigns one.
            let inserted = sqlx::query(
                r#"INSERT INTO "PlayoutItem" (
                    "PlayoutId", "MediaItemId", "Start", "Finish",
                    "GuideStart", "GuideFinish", "CustomTitle", "GuideGroup",
                    "FillerKind", "InPoint", "OutPoint", "ChapterTitle",
                    "DisableWatermarks", "PreferredAudioLanguageCode",
                    "PreferredAudioTitle", "PreferredSubtitleLanguageCode",
                    "SubtitleMode", "MediaVersionId", "VideoStreamIndex",
                    "AudioStreamIndex", "SubtitleStreamIndex", "BlockKey", "CollectionKey",
                    "CollectionEtag", "SchedulingContext"
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(item.playout_id)
            .bind(item.media_item_id)
            .bind(encode_datetime(item.start))
            .bind(encode_datetime(item.finish))
            .bind(encode_optional_datetime(item.guide_start))
            .bind(encode_optional_datetime(item.guide_finish))
            .bind(&item.custom_title)
            .bind(item.guide_group)
            .bind(encode_enum(item.filler_kind))
            .bind(encode_source_duration(item.in_point))
            .bind(encode_source_duration(item.out_point))
            .bind(&item.chapter_title)
            .bind(item.disable_watermarks)
            .bind(&item.preferred_audio_language_code)
            .bind(&item.preferred_audio_title)
            .bind(&item.preferred_subtitle_language_code)
            .bind(item.subtitle_mode.map(encode_enum))
            .bind(item.pinned_stream_selection.map(|selection| selection.media_version_id))
            .bind(item.pinned_stream_selection.map(|selection| selection.video_stream_index))
            .bind(item.pinned_stream_selection.and_then(|selection| selection.audio_stream_index))
            .bind(item.pinned_stream_selection.and_then(|selection| selection.subtitle_stream_index))
            .bind(&item.block_key)
            .bind(&item.collection_key)
            .bind(&item.collection_etag)
            .bind(&item.scheduling_context)
            .execute(&mut *tx)
            .await
            .repository()?;
            let inserted_id = crate::db::last_insert_id(inserted);

            for &wm_id in &item.watermark_ids {
                sqlx::query(
                    r#"REPLACE INTO "PlayoutItemWatermark" ("PlayoutItemId", "WatermarkId") VALUES (?, ?)"#,
                )
                .bind(inserted_id)
                .bind(wm_id)
                .execute(&mut *tx)
                .await.repository()?;
            }
            replace_playout_item_graphics_elements(&mut tx, inserted_id, item).await?;
        }

        // Publish gaps from the final retained/appended item set in the same
        // transaction. Scheduler hints may precede fallback filling or omit
        // gaps entirely, and cannot be used as the persisted UI projection.
        rebuild_playout_gaps(&mut tx, playout.id).await?;

        // ── history ───────────────────────────────────────────────────────────
        if result.clear_items {
            if let Some(seam) = result.retain_through {
                let cutoff = result.remove_before.unwrap_or(seam)
                    - max_playout_offset.max(chrono::Duration::zero());
                sqlx::query(
                    r#"DELETE FROM "PlayoutHistory" WHERE "PlayoutId" = ?
                       AND ("When" >= ? OR "Finish" > ? OR "Finish" < ?)"#,
                )
                .bind(playout.id)
                .bind(encode_datetime(seam))
                .bind(encode_datetime(seam))
                .bind(encode_datetime(cutoff))
                .execute(&mut *tx)
                .await
                .repository()?;
            } else {
                sqlx::query(r#"DELETE FROM "PlayoutHistory" WHERE "PlayoutId" = ?"#)
                    .bind(playout.id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
        }
        // A preserving rebuild can retain a programme's prefix while explicitly
        // discarding orphaned future history inside that same prefix.
        for &history_id in &result.history_to_remove {
            sqlx::query(r#"DELETE FROM "PlayoutHistory" WHERE "Id" = ?"#)
                .bind(history_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        for entry in &result.added_history {
            sqlx::query(
                r#"INSERT INTO "PlayoutHistory" (
                    "BlockId", "Details", "Index", "Key", "PlaybackOrder",
                    "PlayoutId", "When", "Finish", "ChildKey", "IsCurrentChild", "Seed"
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(entry.block_id)
            .bind(&entry.details)
            .bind(entry.index)
            .bind(&entry.key)
            .bind(encode_enum(entry.playback_order))
            .bind(entry.playout_id)
            .bind(encode_datetime(entry.when))
            .bind(encode_optional_datetime(entry.finish))
            .bind(&entry.child_key)
            .bind(entry.is_current_child)
            .bind(entry.seed)
            .execute(&mut *tx)
            .await
            .repository()?;
        }

        // Persist `playout.seed` unconditionally. It only actually changes on a
        // Reset build (see `build_playout` in `maintenance.rs`, which mirrors
        // ErsatzTV's `ResetPlayout(): playout.Seed = new Random().Next()`), but
        // writing it every time keeps this function mode-agnostic and is a
        // no-op for Continue/Refresh, where `playout.seed` is unchanged.
        sqlx::query(r#"UPDATE "Playout" SET "Seed" = ? WHERE "Id" = ?"#)
            .bind(playout.seed)
            .bind(playout.id)
            .execute(&mut *tx)
            .await
            .repository()?;

        // ── anchor, program-schedule anchors, fill-group indices, templates ───
        // Persist the anchor produced by *this* build (`result.anchor`), not the
        // stale anchor carried on the input `playout`. The input anchor is the
        // pre-build state; for a fresh playout it is `None`, and writing it back
        // would delete the row and force every Continue build to restart from
        // `now` — producing overlapping, stacked playout items. ErsatzTV's
        // `PlayoutBuilder` mutates `playout.Anchor` in place before saving; the
        // result anchor is our equivalent of that post-build value.
        replace_anchor_state(&mut tx, playout.id, result.anchor.as_ref()).await?;
        replace_program_schedule_anchors(&mut tx, playout.id, &result.program_schedule_anchors)
            .await?;
        replace_classic_checkpoints(&mut tx, playout.id, &result.classic_checkpoints).await?;
        if playout.schedule_kind == ersatzrs_domain::PlayoutScheduleKind::Sequential {
            yaml_checkpoints::replace(&mut tx, playout.id, &result.yaml_checkpoints).await?;
        }
        if playout.schedule_kind == ersatzrs_domain::PlayoutScheduleKind::Scripted {
            scripted_checkpoints::replace(&mut tx, playout.id, &result.scripted_checkpoints)
                .await?;
            crate::scripted_dependencies::publish(
                &mut tx,
                playout.id,
                &result.scripted_sources,
                result.clear_items,
            )
            .await?;
        }
        replace_fill_group_indices(&mut tx, playout.id, &result.fill_group_indices).await?;
        replace_playout_templates(&mut tx, playout).await?;
        if playout.schedule_kind == ersatzrs_domain::PlayoutScheduleKind::Classic {
            crate::classic_deferred::mark_applied(&mut tx, playout.id, result).await?;
        }

        validate_deadline(result.publication_deadline)?;
        Ok(PendingPlayoutBuild {
            tx,
            playout_id: playout.id,
            channel_id: playout.channel_id,
            deadline: result.publication_deadline,
        })
    }
}
