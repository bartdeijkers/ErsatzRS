//! Fixed borrowed record ranges; original and editor ordinal policies are distinct.

use super::{DrcsTtiBank, Failure, Instant, Page, Result};
use ersatzrs_app::teletext_tti::{TeletextTtiDocument, TeletextTtiRecord};

pub(super) struct Groups<'a> {
    records: [Option<&'a [TeletextTtiRecord]>; 16],
    offset: u64,
}
impl<'a> Groups<'a> {
    pub(super) fn index(
        document: &'a TeletextTtiDocument,
        bank: DrcsTtiBank,
        edited: bool,
        deadline: Instant,
    ) -> Result<Self> {
        let location = bank.location();
        location.check_deadline(deadline)?;
        let records = document.records();
        let mut groups = [None; 16];
        let mut count = 0;
        let mut start = None;
        for (index, record) in records.iter().enumerate() {
            location.check_deadline(deadline)?;
            if matches!(record, TeletextTtiRecord::PageNumber { .. }) {
                if let Some(start) = start {
                    groups[count - 1] = Some(&records[start..index]);
                } else if index != 0 {
                    return Err(bank.group_location(0).error(Failure::Structure));
                }
                if count == bank.len {
                    return Err(bank.group_location(count).error(Failure::Identity));
                }
                start = Some(index);
                count += 1;
            }
        }
        if count != bank.len {
            return Err(bank.group_location(count).error(Failure::Identity));
        }
        groups[count - 1] =
            Some(&records[start.ok_or_else(|| location.error(Failure::Identity))?..]);
        let Some(TeletextTtiRecord::PageNumber { editor_ordinal, .. }) = records.first() else {
            return Err(location.error(Failure::Identity));
        };
        let offset = if edited && bank.len > 1 && *editor_ordinal <= 1 {
            *editor_ordinal
        } else {
            0
        };
        let value = Self {
            records: groups,
            offset,
        };
        // Check every identity before comparing any bitmap content. One global
        // offset cannot conceal missing, reordered or independently shifted S1.
        for index in 0..bank.len {
            value.page(index, bank, deadline)?;
        }
        Ok(value)
    }
    pub(super) fn page(
        &self,
        index: usize,
        bank: DrcsTtiBank,
        deadline: Instant,
    ) -> Result<Page<'a>> {
        let location = bank.group_location(index);
        let records = self
            .records
            .get(index)
            .copied()
            .flatten()
            .ok_or_else(|| location.error(Failure::Identity))?;
        let ordinal = u64::try_from(index).map_err(|_| location.error(Failure::Mapping))?;
        Page::index_records(
            records,
            location.page,
            ordinal + self.offset,
            location,
            deadline,
        )
    }
}
