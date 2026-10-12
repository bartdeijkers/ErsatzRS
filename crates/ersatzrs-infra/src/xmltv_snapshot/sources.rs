use super::{
    admitted, allocation, failure, rows, CapturedGuideChannel, GuideSnapshotError,
    GuideSnapshotFailure, GuideSnapshotPathAssociation, GuideSnapshotSource,
};
use crate::{
    db::Db,
    external_json::snapshot::{checked_accepted_on, AcceptedSnapshotBudget},
    xmltv_read::{GuideReadBudget, Read},
};
use chrono::Duration;
use ersatzrs_core::{programme::ProgrammeGuideItem, xmltv::XmltvGuideItem};
use ersatzrs_domain::{FillerKind, PlayoutScheduleKind};
use sqlx::Row;

const ITEM_SQL: &str = r#"SELECT
 c."Number" AS "Number",c."PlayoutSource" AS "PlayoutSource",c."PlayoutOffset" AS "PlayoutOffset",
 p."Id" AS "PlayoutId",p."ScheduleKind" AS "ScheduleKind",pi."Id" AS "ItemId",pi."MediaItemId" AS "MediaItemId",
 pi."InPoint" AS "InPoint",pi."OutPoint" AS "OutPoint",pi."Start" AS "Start",pi."Finish" AS "Finish",
 pi."GuideStart" AS "GuideStart",pi."GuideFinish" AS "GuideFinish",pi."GuideGroup" AS "GuideGroup",
 pi."FillerKind" AS "FillerKind",pi."CustomTitle" AS "CustomTitle"
 FROM "Channel" c JOIN "Playout" p ON p."ChannelId"=CASE WHEN c."PlayoutSource"=1 THEN c."MirrorSourceChannelId" ELSE c."Id" END
 JOIN "PlayoutItem" pi ON pi."PlayoutId"=p."Id"
 WHERE p."ScheduleKind" NOT IN (20,'ExternalJson') AND c."Id"=? AND p."Id"=?
 ORDER BY c."Number",p."Id",pi."Start",pi."Id""#;
const ITEM_COLUMNS: &[&str] = &[
    "Number",
    "PlayoutSource",
    "PlayoutOffset",
    "PlayoutId",
    "ScheduleKind",
    "ItemId",
    "MediaItemId",
    "InPoint",
    "OutPoint",
    "Start",
    "Finish",
    "GuideStart",
    "GuideFinish",
    "GuideGroup",
    "FillerKind",
    "CustomTitle",
];

pub(super) async fn load(
    transaction: &mut sqlx::Transaction<'_, Db>,
    budget: &mut GuideReadBudget,
    channel: &mut CapturedGuideChannel,
    source_channel: i64,
) -> Result<Vec<ProgrammeGuideItem>, GuideSnapshotError> {
    let playouts=rows(transaction,budget,r#"SELECT "Id","ScheduleKind","ScheduleFile" FROM "Playout" WHERE "ChannelId"=? ORDER BY "Id""#,
        &[source_channel],&["Id","ScheduleKind","ScheduleFile"],Some(channel.id),None).await?;
    admitted(
        allocation::slots::<GuideSnapshotSource>(budget, playouts.len()),
        Some(channel.id),
        None,
    )?;
    admitted(
        allocation::path_associations(budget, playouts.len()),
        Some(channel.id),
        None,
    )?;
    channel.path_associations = Vec::with_capacity(playouts.len());
    let mut items = Vec::new();
    for row in playouts {
        let id = admitted(row.try_get("Id"), Some(channel.id), None)?;
        let kind = crate::repositories::common::decode_enum_column::<PlayoutScheduleKind>(
            "guide schedule kind",
            &row,
            "ScheduleKind",
        )
        .map_err(|_| {
            failure(
                Some(channel.id),
                Some(id),
                GuideSnapshotFailure::InvalidValue,
            )
        })?;
        let configured_schedule_file =
            admitted(row.try_get("ScheduleFile"), Some(channel.id), Some(id))?;
        let (incoming, receipt, accepted_schedule_file) =
            if kind == PlayoutScheduleKind::ExternalJson {
                let (incoming, receipt, path) = external(transaction, budget, channel, id).await?;
                (incoming, receipt, Some(path))
            } else {
                let incoming = sql(transaction, budget, channel.id, id).await?;
                let receipt = GuideSnapshotSource::Sql {
                    playout_id: id,
                    schedule_kind: kind,
                    raw_rows: incoming.len(),
                };
                (incoming, receipt, None)
            };
        admitted(
            allocation::slots::<ProgrammeGuideItem>(budget, incoming.len()),
            Some(channel.id),
            Some(id),
        )?;
        items.extend(incoming);
        channel.sources.push(receipt);
        channel
            .path_associations
            .push(GuideSnapshotPathAssociation {
                playout_id: id,
                configured_schedule_file,
                accepted_schedule_file,
            });
    }
    Ok(items)
}

async fn sql(
    transaction: &mut sqlx::Transaction<'_, Db>,
    budget: &mut GuideReadBudget,
    channel: i64,
    playout: i64,
) -> Result<Vec<ProgrammeGuideItem>, GuideSnapshotError> {
    let source = rows(
        transaction,
        budget,
        ITEM_SQL,
        &[channel, playout],
        ITEM_COLUMNS,
        Some(channel),
        Some(playout),
    )
    .await?;
    admitted(
        allocation::slots::<i64>(budget, source.len()),
        Some(channel),
        Some(playout),
    )?;
    let mut ids = source
        .iter()
        .map(|row| admitted(row.try_get("MediaItemId"), Some(channel), Some(playout)))
        .collect::<Result<Vec<i64>, _>>()?;
    ids.sort_unstable();
    ids.dedup();
    let content = admitted(
        crate::xmltv_metadata::load_content_cache_with_reader(
            &mut Read::snapshot(transaction, budget),
            &ids,
        )
        .await,
        Some(channel),
        Some(playout),
    )?;
    admitted(
        allocation::slots::<ProgrammeGuideItem>(budget, source.len()),
        Some(channel),
        Some(playout),
    )?;
    let mut items = Vec::with_capacity(source.len());
    for row in source {
        validate_times(&row, channel, playout)?;
        let media: i64 = admitted(row.try_get("MediaItemId"), Some(channel), Some(playout))?;
        if let Some((title, metadata)) = content.get(&media) {
            let mut cost = allocation::Cost::default();
            if let Some(title) = title {
                admitted(cost.add(1, title.len()), Some(channel), Some(playout))?;
            }
            admitted(cost.content(metadata), Some(channel), Some(playout))?;
            admitted(cost.reserve(budget), Some(channel), Some(playout))?;
        }
        items.push(
            crate::xmltv::materialize_guide_row(&row, &content).map_err(|_| {
                failure(
                    Some(channel),
                    Some(playout),
                    GuideSnapshotFailure::InvalidValue,
                )
            })?,
        );
    }
    Ok(items)
}

fn validate_times(
    row: &crate::db::DbRow,
    channel: i64,
    playout: i64,
) -> Result<(), GuideSnapshotError> {
    for column in ["Start", "Finish"] {
        let text: &str = admitted(row.try_get(column), Some(channel), Some(playout))?;
        if crate::xmltv::parse_datetime(text).is_none() {
            return Err(failure(
                Some(channel),
                Some(playout),
                GuideSnapshotFailure::InvalidValue,
            ));
        }
    }
    Ok(())
}

async fn external(
    transaction: &mut sqlx::Transaction<'_, Db>,
    budget: &mut GuideReadBudget,
    channel: &CapturedGuideChannel,
    playout: i64,
) -> Result<(Vec<ProgrammeGuideItem>, GuideSnapshotSource, String), GuideSnapshotError> {
    let remaining = budget.remaining_source_limits();
    if remaining.max_records < 1 {
        return Err(failure(
            Some(channel.id),
            Some(playout),
            GuideSnapshotFailure::Admission,
        ));
    }
    let checked = checked_accepted_on(
        transaction.as_mut(),
        playout,
        AcceptedSnapshotBudget {
            max_document_bytes: remaining.max_source_bytes,
            max_path_bytes: remaining.max_source_bytes,
            max_total_bytes: remaining.max_source_bytes,
            max_programs: remaining.max_records - 1,
        },
    )
    .await
    .map_err(|error| {
        failure(
            Some(channel.id),
            Some(playout),
            GuideSnapshotFailure::Accepted(error),
        )
    })?
    .ok_or_else(|| {
        failure(
            Some(channel.id),
            Some(playout),
            GuideSnapshotFailure::MissingAccepted,
        )
    })?;
    let usage = checked.usage();
    let records = usage.programs.checked_add(1).ok_or_else(|| {
        failure(
            Some(channel.id),
            Some(playout),
            GuideSnapshotFailure::Admission,
        )
    })?;
    let bytes = usage
        .document_bytes
        .checked_add(usage.path_bytes)
        .ok_or_else(|| {
            failure(
                Some(channel.id),
                Some(playout),
                GuideSnapshotFailure::Admission,
            )
        })?;
    admitted(
        budget.charge_source(records, bytes),
        Some(channel.id),
        Some(playout),
    )?;
    admitted(
        allocation::accepted_parse(budget, usage.programs, usage.document_bytes),
        Some(channel.id),
        Some(playout),
    )?;
    let document_digest = checked.document_digest();
    let snapshot = checked
        .parse()
        .map_err(|error| {
            failure(
                Some(channel.id),
                Some(playout),
                GuideSnapshotFailure::Accepted(error),
            )
        })?
        .into_snapshot();
    let receipt = GuideSnapshotSource::Accepted {
        playout_id: playout,
        revision: snapshot.revision,
        raw_programs: usage.programs,
        document_digest,
    };
    admitted(
        allocation::slots::<ProgrammeGuideItem>(budget, snapshot.schedule.items.len()),
        Some(channel.id),
        Some(playout),
    )?;
    let minimum = i64::from(snapshot.schedule.guide_minimum_duration_seconds) * 1000;
    let mut items = Vec::new();
    for (index, item) in snapshot.schedule.items.into_iter().enumerate() {
        if i64::from(item.program.duration) < minimum {
            continue;
        }
        items.push(external_item(budget, channel, playout, index, item)?);
    }
    Ok((items, receipt, snapshot.schedule_file))
}

fn external_item(
    budget: &mut GuideReadBudget,
    channel: &CapturedGuideChannel,
    playout: i64,
    index: usize,
    item: crate::external_json::ExternalJsonItem,
) -> Result<ProgrammeGuideItem, GuideSnapshotError> {
    let invalid = || {
        failure(
            Some(channel.id),
            Some(playout),
            GuideSnapshotFailure::InvalidValue,
        )
    };
    let start = item
        .start
        .checked_add_signed(channel.playout_offset)
        .ok_or_else(invalid)?;
    let finish = item
        .finish
        .checked_add_signed(channel.playout_offset)
        .ok_or_else(invalid)?;
    let title_bytes = if item.program.kind.as_deref() == Some("episode") {
        0
    } else {
        item.program.title.as_ref().map_or(0, String::len)
    };
    let mut cost = allocation::Cost::default();
    admitted(
        cost.add(1, channel.number.len()),
        Some(channel.id),
        Some(playout),
    )?;
    if item.program.kind.as_deref() != Some("episode") && item.program.title.is_some() {
        admitted(cost.add(1, title_bytes), Some(channel.id), Some(playout))?;
    }
    admitted(cost.reserve(budget), Some(channel.id), Some(playout))?;
    let (title, content) = crate::external_json::guide::content(item.program, true);
    Ok(ProgrammeGuideItem {
        playout_id: playout,
        item_id: i64::try_from(index).map_err(|_| invalid())?,
        media_item_id: None,
        schedule_kind: PlayoutScheduleKind::ExternalJson,
        in_point: Duration::zero(),
        out_point: finish - start,
        guide: XmltvGuideItem {
            channel_number: channel.number.clone(),
            start,
            finish,
            guide_start: None,
            guide_finish: None,
            guide_group: i32::try_from(index).map_err(|_| invalid())?,
            filler_kind: FillerKind::None,
            custom_title: None,
            title,
            content,
        },
    })
}
