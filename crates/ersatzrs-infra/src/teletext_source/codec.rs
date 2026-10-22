use ersatzrs_core::repositories::Result;
use ersatzrs_domain::{
    MediaSourceKind, SourceTeletextBinding, SourceTeletextBindingIdentity,
    SourceTeletextDescriptor, SourceTeletextDescriptors, SourceTeletextObservation,
    SourceTeletextPid, SourceTeletextProgram, SourceTeletextPrograms, SourceTeletextProvenance,
    SourceTeletextRevision, TeletextLanguage,
};
use sqlx::Transaction;

use super::{
    column, invalid, storage, SourceTeletextParents, MAX_SOURCE_TELETEXT_DESCRIPTORS,
    MAX_SOURCE_TELETEXT_PROGRAMS,
};
use crate::db::{Db, DbRow};

pub(super) fn validate(observation: &SourceTeletextObservation) -> Result<()> {
    // Bound vectors before domain validation allocates its ordinal set.
    if matches!(&observation.programs, SourceTeletextPrograms::Reported(entries) if entries.len() > MAX_SOURCE_TELETEXT_PROGRAMS)
        || matches!(&observation.descriptors, SourceTeletextDescriptors::Reported { entries, .. } if entries.len() > MAX_SOURCE_TELETEXT_DESCRIPTORS)
    {
        return Err(invalid("collection resource bound"));
    }
    observation
        .validate()
        .map_err(|_| invalid("observation structure"))?;
    if observation.provenance == SourceTeletextProvenance::Unavailable
        && (matches!(observation.pid, SourceTeletextPid::Reported(_))
            || matches!(observation.programs, SourceTeletextPrograms::Reported(_))
            || matches!(
                observation.descriptors,
                SourceTeletextDescriptors::Reported { .. }
            ))
    {
        return Err(invalid("reported metadata provenance"));
    }
    Ok(())
}

pub(super) struct Encoded<'a> {
    pub binding_status: i32,
    pub source_id: Option<i64>,
    pub provider: Option<&'static str>,
    pub revision: Option<&'a str>,
    pub provenance: i32,
    pub pid_status: i32,
    pub pid: Option<i32>,
    pub program_status: i32,
    pub descriptor_status: i32,
    pub trailing_unpaired_pairs: Option<i64>,
}

pub(super) fn encode(observation: &SourceTeletextObservation) -> Encoded<'_> {
    let (binding_status, source_id, provider, revision) = match &observation.binding {
        SourceTeletextBinding::Unavailable => (0, None, None, None),
        SourceTeletextBinding::Unusable => (1, None, None, None),
        SourceTeletextBinding::Bound(identity) => (
            2,
            Some(identity.media_source_id),
            Some(identity.provider.provider_name()),
            Some(identity.source_revision.as_str()),
        ),
    };
    let (pid_status, pid) = match observation.pid {
        SourceTeletextPid::Unavailable => (0, None),
        SourceTeletextPid::Unusable => (1, None),
        SourceTeletextPid::Reported(pid) => (2, Some(i32::from(pid))),
    };
    let program_status = match observation.programs {
        SourceTeletextPrograms::Unavailable => 0,
        SourceTeletextPrograms::Unusable => 1,
        SourceTeletextPrograms::Reported(_) => 2,
    };
    let (descriptor_status, trailing_unpaired_pairs) = match &observation.descriptors {
        SourceTeletextDescriptors::Unavailable => (0, None),
        SourceTeletextDescriptors::Unusable => (1, None),
        SourceTeletextDescriptors::Reported {
            trailing_unpaired_pairs,
            ..
        } => (2, trailing_unpaired_pairs.map(i64::from)),
    };
    Encoded {
        binding_status,
        source_id,
        provider,
        revision,
        provenance: match observation.provenance {
            SourceTeletextProvenance::Unavailable => 0,
            SourceTeletextProvenance::Ffprobe9PackedDescriptorProjection => 1,
        },
        pid_status,
        pid,
        program_status,
        descriptor_status,
        trailing_unpaired_pairs,
    }
}

pub(super) async fn replace_children(
    tx: &mut Transaction<'_, Db>,
    id: i64,
    observation: &SourceTeletextObservation,
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "SourceTeletextDescriptor" WHERE "SourceTeletextMetadataId"=?"#)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|_| storage())?;
    sqlx::query(r#"DELETE FROM "SourceTeletextProgram" WHERE "SourceTeletextMetadataId"=?"#)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|_| storage())?;
    if let SourceTeletextDescriptors::Reported { entries, .. } = &observation.descriptors {
        for entry in entries {
            sqlx::query(r#"INSERT INTO "SourceTeletextDescriptor" ("SourceTeletextMetadataId", "ProjectionOrdinal", "Language", "DescriptorType", "WireMagazine", "WirePage") VALUES (?, ?, ?, ?, ?, ?)"#)
                .bind(id).bind(i64::from(entry.projection_ordinal)).bind(entry.language.as_str())
                .bind(i32::from(entry.descriptor_type)).bind(i32::from(entry.wire_magazine)).bind(i32::from(entry.wire_page))
                .execute(&mut **tx).await.map_err(|_| storage())?;
        }
    }
    if let SourceTeletextPrograms::Reported(entries) = &observation.programs {
        for (ordinal, entry) in entries.iter().enumerate() {
            sqlx::query(r#"INSERT INTO "SourceTeletextProgram" ("SourceTeletextMetadataId", "MembershipOrdinal", "ProgramId", "ProgramNumber", "PmtPid", "PcrPid") VALUES (?, ?, ?, ?, ?, ?)"#)
                .bind(id).bind(i64::try_from(ordinal).map_err(|_| invalid("membership ordinal"))?)
                .bind(i64::from(entry.program_id)).bind(i64::from(entry.program_number))
                .bind(i32::from(entry.pmt_pid)).bind(i32::from(entry.pcr_pid))
                .execute(&mut **tx).await.map_err(|_| storage())?;
        }
    }
    super::header_codec::replace(tx, id, &observation.observed_pages).await?;
    super::page_codec::replace(tx, id, &observation.observed_pages).await?;
    Ok(())
}

pub(super) async fn decode(
    tx: &mut Transaction<'_, Db>,
    row: &DbRow,
    parents: SourceTeletextParents,
) -> Result<SourceTeletextObservation> {
    if column::<i64>(row, "ObservedPagesStatus")? != 0 {
        return Err(invalid("observed pages status"));
    }
    let provenance = match column::<i64>(row, "Provenance")? {
        0 => SourceTeletextProvenance::Unavailable,
        1 => SourceTeletextProvenance::Ffprobe9PackedDescriptorProjection,
        _ => return Err(invalid("stored provenance")),
    };
    let pid_value: Option<i64> = column(row, "Pid")?;
    let pid = match (column::<i64>(row, "PidStatus")?, pid_value) {
        (0, None) => SourceTeletextPid::Unavailable,
        (1, None) => SourceTeletextPid::Unusable,
        (2, Some(pid)) => {
            SourceTeletextPid::Reported(u16::try_from(pid).map_err(|_| invalid("stored PID"))?)
        }
        _ => return Err(invalid("stored PID status")),
    };
    let id: i64 = column(row, "Id")?;
    if id <= 0 {
        return Err(invalid("stored metadata identity"));
    }
    let headers = super::header_codec::load(tx, id).await?;
    let observed_pages = super::page_codec::load(tx, id, headers).await?;
    let observation = SourceTeletextObservation {
        binding: decode_binding(row, parents)?,
        stream_index: i32::try_from(column::<i64>(row, "StreamIndex")?)
            .map_err(|_| invalid("stored stream index"))?,
        provenance,
        pid,
        programs: decode_programs(tx, id, column(row, "ProgramStatus")?).await?,
        descriptors: decode_descriptors(
            tx,
            id,
            column(row, "DescriptorStatus")?,
            column(row, "TrailingUnpairedPairs")?,
        )
        .await?,
        observed_pages,
    };
    validate(&observation)?;
    Ok(observation)
}

fn decode_binding(row: &DbRow, parents: SourceTeletextParents) -> Result<SourceTeletextBinding> {
    let source_id: Option<i64> = column(row, "MediaSourceId")?;
    let provider: Option<String> = column(row, "Provider")?;
    let revision: Option<String> = column(row, "SourceRevision")?;
    match (
        column::<i64>(row, "BindingStatus")?,
        source_id,
        provider,
        revision,
    ) {
        (0, None, None, None) => Ok(SourceTeletextBinding::Unavailable),
        (1, None, None, None) => Ok(SourceTeletextBinding::Unusable),
        (2, Some(media_source_id), Some(provider), Some(revision)) => {
            let provider = MediaSourceKind::ALL
                .into_iter()
                .find(|kind| kind.provider_name() == provider)
                .ok_or_else(|| invalid("stored provider"))?;
            let source_revision =
                SourceTeletextRevision::new(revision).map_err(|_| invalid("stored revision"))?;
            Ok(SourceTeletextBinding::Bound(
                SourceTeletextBindingIdentity {
                    media_source_id,
                    provider,
                    media_version_id: parents.media_version_id,
                    media_stream_id: parents.media_stream_id,
                    media_file_id: parents.media_file_id,
                    source_revision,
                },
            ))
        }
        _ => Err(invalid("stored binding status")),
    }
}

async fn decode_descriptors(
    tx: &mut Transaction<'_, Db>,
    id: i64,
    status: i64,
    warning: Option<i64>,
) -> Result<SourceTeletextDescriptors> {
    let rows = sqlx::query(
        r#"SELECT "ProjectionOrdinal", CASE WHEN TYPEOF("Language")='text' AND LENGTH(CAST("Language" AS BLOB))=3 THEN "Language" ELSE NULL END AS "Language", "DescriptorType", "WireMagazine", "WirePage"
        FROM "SourceTeletextDescriptor" WHERE "SourceTeletextMetadataId"=? ORDER BY "ProjectionOrdinal" LIMIT ?"#,
    ).bind(id).bind(i64::try_from(MAX_SOURCE_TELETEXT_DESCRIPTORS + 1).map_err(|_| invalid("descriptor resource bound"))?)
        .fetch_all(&mut **tx).await.map_err(|_| storage())?;
    if rows.len() > MAX_SOURCE_TELETEXT_DESCRIPTORS {
        return Err(invalid("descriptor resource bound"));
    }
    match status {
        0 if rows.is_empty() && warning.is_none() => {
            return Ok(SourceTeletextDescriptors::Unavailable)
        }
        1 if rows.is_empty() && warning.is_none() => {
            return Ok(SourceTeletextDescriptors::Unusable)
        }
        2 => {}
        _ => return Err(invalid("descriptor status children")),
    }
    let entries = rows
        .iter()
        .map(|row| {
            Ok(SourceTeletextDescriptor {
                projection_ordinal: unsigned(row, "ProjectionOrdinal")?,
                language: TeletextLanguage::new(column(row, "Language")?)
                    .map_err(|_| invalid("stored language"))?,
                descriptor_type: unsigned(row, "DescriptorType")?,
                wire_magazine: unsigned(row, "WireMagazine")?,
                wire_page: unsigned(row, "WirePage")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(SourceTeletextDescriptors::Reported {
        entries,
        trailing_unpaired_pairs: warning
            .map(|count| u32::try_from(count).map_err(|_| invalid("stored warning")))
            .transpose()?,
    })
}

async fn decode_programs(
    tx: &mut Transaction<'_, Db>,
    id: i64,
    status: i64,
) -> Result<SourceTeletextPrograms> {
    let rows = sqlx::query(r#"SELECT * FROM "SourceTeletextProgram" WHERE "SourceTeletextMetadataId"=? ORDER BY "MembershipOrdinal" LIMIT ?"#)
        .bind(id).bind(i64::try_from(MAX_SOURCE_TELETEXT_PROGRAMS + 1).map_err(|_| invalid("program resource bound"))?)
        .fetch_all(&mut **tx).await.map_err(|_| storage())?;
    if rows.len() > MAX_SOURCE_TELETEXT_PROGRAMS {
        return Err(invalid("program resource bound"));
    }
    match status {
        0 if rows.is_empty() => return Ok(SourceTeletextPrograms::Unavailable),
        1 if rows.is_empty() => return Ok(SourceTeletextPrograms::Unusable),
        2 => {}
        _ => return Err(invalid("program status children")),
    }
    let mut programs = Vec::with_capacity(rows.len());
    for (ordinal, row) in rows.iter().enumerate() {
        if column::<i64>(row, "MembershipOrdinal")?
            != i64::try_from(ordinal).map_err(|_| invalid("membership ordinal"))?
        {
            return Err(invalid("membership ordering"));
        }
        programs.push(SourceTeletextProgram {
            program_id: unsigned(row, "ProgramId")?,
            program_number: unsigned(row, "ProgramNumber")?,
            pmt_pid: unsigned(row, "PmtPid")?,
            pcr_pid: unsigned(row, "PcrPid")?,
        });
    }
    Ok(SourceTeletextPrograms::Reported(programs))
}

fn unsigned<T: TryFrom<i64>>(row: &DbRow, name: &str) -> Result<T> {
    T::try_from(column::<i64>(row, name)?).map_err(|_| invalid("stored unsigned value"))
}
