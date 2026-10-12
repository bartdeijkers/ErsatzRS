//! Conservative modeled allocation admission before copies and core projection.
//! Counts exclude allocator/container overhead and parser scratch; this is not RSS.

use crate::{
    external_json::{ExternalJsonItem, ExternalJsonProgram},
    xmltv_read::GuideReadBudget,
};
use chrono::{DateTime, Utc};
use ersatzrs_core::{
    programme::{ProgrammeGuideItem, ProgrammeOccurrence, ProgrammeSegment},
    xmltv::{XmltvGuideItem, XmltvProgramme, XmltvProgrammeContent},
};
use std::mem::size_of;

#[derive(Default)]
pub(super) struct Cost {
    pub nodes: usize,
    pub bytes: usize,
}
fn arithmetic() -> sqlx::Error {
    sqlx::Error::Decode("guide snapshot allocation arithmetic overflow".into())
}
impl Cost {
    pub fn add(&mut self, nodes: usize, bytes: usize) -> Result<(), sqlx::Error> {
        self.nodes = self.nodes.checked_add(nodes).ok_or_else(arithmetic)?;
        self.bytes = self.bytes.checked_add(bytes).ok_or_else(arithmetic)?;
        Ok(())
    }
    pub fn slots<T>(&mut self, count: usize) -> Result<(), sqlx::Error> {
        self.add(
            count,
            count.checked_mul(size_of::<T>()).ok_or_else(arithmetic)?,
        )
    }
    fn string(&mut self, value: &str) -> Result<(), sqlx::Error> {
        self.add(1, value.len())
    }
    fn optional(&mut self, value: Option<&str>) -> Result<(), sqlx::Error> {
        if let Some(value) = value {
            self.string(value)?;
        }
        Ok(())
    }
    fn strings(&mut self, values: &[String]) -> Result<(), sqlx::Error> {
        self.slots::<String>(values.len())?;
        for value in values {
            self.bytes = self.bytes.checked_add(value.len()).ok_or_else(arithmetic)?;
        }
        Ok(())
    }
    pub fn content(&mut self, content: &XmltvProgrammeContent) -> Result<(), sqlx::Error> {
        let XmltvProgrammeContent {
            kind: _,
            sub_title,
            description,
            year: _,
            genres,
            tags,
            artist_genres,
            content_rating,
            icon_artwork,
            icon_size_hint: _,
            thumbnail_artwork,
            thumbnail_size_hint: _,
            season_number: _,
            episode_number: _,
            guids,
            episode_guids,
            track,
            album,
            release_date,
            collection,
            people,
            original_broadcasters,
            broadcasters,
            artists,
            album_artists,
            comment,
            studios,
            directors,
        } = content;
        for value in [
            sub_title,
            description,
            content_rating,
            track,
            album,
            release_date,
            collection,
            comment,
        ] {
            self.optional(value.as_deref())?;
        }
        for values in [
            genres,
            tags,
            artist_genres,
            guids,
            episode_guids,
            original_broadcasters,
            broadcasters,
            artists,
            album_artists,
            studios,
            directors,
        ] {
            self.strings(values)?;
        }
        for artwork in [icon_artwork, thumbnail_artwork].into_iter().flatten() {
            self.string(&artwork.path)?;
        }
        self.slots::<ersatzrs_core::xmltv::XmltvCredit>(people.len())?;
        for person in people {
            self.string(&person.name)?;
            self.string(&person.role)?;
        }
        Ok(())
    }
    pub fn reserve(self, budget: &mut GuideReadBudget) -> Result<(), sqlx::Error> {
        budget.reserve_expansion(self.nodes, self.bytes)
    }
}

pub(super) fn slots<T>(budget: &mut GuideReadBudget, count: usize) -> Result<(), sqlx::Error> {
    let mut cost = Cost::default();
    cost.slots::<T>(count)?;
    cost.reserve(budget)
}

pub(super) fn path_associations(
    budget: &mut GuideReadBudget,
    count: usize,
) -> Result<(), sqlx::Error> {
    // Payload strings move from source-admitted rows/snapshots. Model each
    // additional record and both String headers before allocating the vector.
    slots::<super::GuideSnapshotPathAssociation>(budget, count)
}

pub(super) fn accepted_parse(
    budget: &mut GuideReadBudget,
    programs: usize,
    document_bytes: usize,
) -> Result<(), sqlx::Error> {
    let nodes = programs
        .checked_mul(11)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(arithmetic)?;
    let slots = size_of::<ExternalJsonProgram>()
        .checked_add(size_of::<ExternalJsonItem>())
        .ok_or_else(arithmetic)?;
    let bytes = programs
        .checked_mul(slots)
        .and_then(|n| n.checked_add(document_bytes))
        .ok_or_else(arithmetic)?;
    // Both vectors, nine optional String buffers per program, and startTime.
    // All decoded payload bytes are conservatively bounded by the original JSON.
    budget.reserve_expansion(nodes, bytes)
}

pub(super) fn core_projection(
    budget: &mut GuideReadBudget,
    items: &[ProgrammeGuideItem],
) -> Result<(), sqlx::Error> {
    let mut cost = Cost::default();
    let n = items.len();
    cost.slots::<((&str, i64), Vec<&ProgrammeGuideItem>)>(n)?;
    cost.slots::<(
        (DateTime<Utc>, DateTime<Utc>, i32),
        Vec<&ProgrammeGuideItem>,
    )>(n)?;
    cost.slots::<(
        (String, DateTime<Utc>, DateTime<Utc>, i32),
        Vec<&XmltvGuideItem>,
    )>(n)?;
    // Outer, block and split reference vectors plus reference-sort scratch.
    for _ in 0..4 {
        cost.slots::<&ProgrammeGuideItem>(n)?;
    }
    cost.slots::<XmltvGuideItem>(n)?;
    cost.slots::<Vec<ProgrammeSegment>>(n)?;
    cost.slots::<(i64, i64)>(n)?;
    cost.slots::<XmltvProgramme>(n)?;
    // Grouped/block/final output vectors and final stable-sort scratch.
    for _ in 0..4 {
        cost.slots::<ProgrammeOccurrence>(n)?;
    }
    for _ in 0..2 {
        cost.slots::<ProgrammeSegment>(n)?;
    }
    for item in items {
        for _ in 0..2 {
            cost.string(&item.guide.title)?;
            cost.optional(item.guide.custom_title.as_deref())?;
            cost.content(&item.guide.content)?;
        }
        for _ in 0..3 {
            cost.string(&item.guide.channel_number)?;
        }
    }
    cost.reserve(budget)
}
