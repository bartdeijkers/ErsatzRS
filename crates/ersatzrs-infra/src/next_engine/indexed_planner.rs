use std::{collections::BTreeMap, io::Write};

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::programme::ProgrammeClock;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{
    contract::{INDEX_VERSION, MAX_DAYS, MAX_DAY_BYTES, MAX_INDEX_BYTES, MAX_TOTAL_BYTES},
    DayReference, PlayoutIndex, PublicationError,
};
use crate::next_engine::protocol::{Playout, PlayoutItem};

pub(super) struct Existing {
    pub index: PlayoutIndex,
    pub live: BTreeMap<DateTime<Utc>, Option<String>>,
}

pub(super) struct Plan {
    pub index: PlayoutIndex,
    pub chunks: Vec<(DayReference, Vec<u8>)>,
}

#[derive(Serialize)]
struct DayDocument<'a> {
    version: &'a str,
    items: Vec<&'a PlayoutItem>,
}

pub(super) fn plan(
    playout: &Playout,
    old: Option<&Existing>,
    clock: &ProgrammeClock,
    now: DateTime<Utc>,
    is_current: &impl Fn() -> bool,
    deadline: Option<std::time::Instant>,
) -> Result<Option<Plan>, PublicationError> {
    super::check(is_current, deadline)?;
    let cutoff = now
        .checked_sub_signed(Duration::minutes(5))
        .ok_or(PublicationError::InvalidWindow)?;
    let mut days = BTreeMap::<_, (DateTime<Utc>, Vec<&PlayoutItem>)>::new();
    let mut references = 0usize;
    for item in playout.items.iter().filter(|item| item.finish > cutoff) {
        let mut cursor = item.start.max(cutoff);
        while cursor < item.finish {
            super::check(is_current, deadline)?;
            let (start, finish) = clock
                .day_bounds(cursor)
                .map_err(|_| PublicationError::InvalidWindow)?;
            if finish <= cursor {
                return Err(PublicationError::InvalidWindow);
            }
            days.entry(start)
                .or_insert_with(|| (finish, Vec::new()))
                .1
                .push(item);
            if days.len() > MAX_DAYS {
                return Err(PublicationError::Limit);
            }
            references += 1;
            if references > MAX_TOTAL_BYTES / 64 {
                return Err(PublicationError::Limit);
            }
            cursor = finish;
        }
    }
    let mut index = PlayoutIndex {
        version: INDEX_VERSION.into(),
        days: Vec::new(),
    };
    let mut chunks = Vec::new();
    let mut live = BTreeMap::new();
    let mut total = 0;
    for (start, (finish, items)) in days {
        super::check(is_current, deadline)?;
        let document = DayDocument {
            version: &playout.version,
            items,
        };
        let bytes = encode(&document, MAX_DAY_BYTES.min(MAX_TOTAL_BYTES - total))?;
        total += bytes.len();
        let hash = digest(&bytes);
        live.insert(start, Some(hash.clone()));
        if let Some(reference) = old.and_then(|old| {
            (old.live.get(&start) == Some(&Some(hash.clone())))
                .then(|| {
                    old.index
                        .days
                        .iter()
                        .find(|day| day.start == start && day.finish == finish)
                })
                .flatten()
        }) {
            index.days.push(reference.clone());
        } else {
            let reference = DayReference {
                start,
                finish,
                file: format!("day-{hash}.json"),
                sha256: hash,
            };
            index.days.push(reference.clone());
            chunks.push((reference, bytes));
        }
    }
    if let Some(old) = old {
        // A no-op retains old finished rows and the exact index/chunk metadata.
        // Explicit empty input still withdraws all previous index entries.
        let same_live = old
            .live
            .iter()
            .filter(|(_, hash)| hash.is_some())
            .eq(live.iter());
        if same_live
            && chunks.is_empty()
            && (!playout.items.is_empty() || old.index.days.is_empty())
        {
            return Ok(None);
        }
    }
    index.validate()?;
    let _ = encode(&index, MAX_INDEX_BYTES)?;
    Ok(Some(Plan { index, chunks }))
}

pub(super) fn live_hash(
    playout: &Playout,
    now: DateTime<Utc>,
) -> Result<Option<String>, PublicationError> {
    let cutoff = now
        .checked_sub_signed(Duration::minutes(5))
        .ok_or(PublicationError::InvalidWindow)?;
    let items = playout
        .items
        .iter()
        .filter(|item| item.finish > cutoff)
        .collect::<Vec<_>>();
    if items.is_empty() {
        return Ok(None);
    }
    let document = DayDocument {
        version: &playout.version,
        items,
    };
    Ok(Some(digest(&encode(&document, MAX_DAY_BYTES)?)))
}

pub(super) fn digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

pub(super) fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, PublicationError> {
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
        exceeded: bool,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                self.exceeded = true;
                return Err(std::io::ErrorKind::FileTooLarge.into());
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded {
        bytes: Vec::new(),
        limit,
        exceeded: false,
    };
    let result = serde_json::to_writer(&mut output, value);
    if output.exceeded {
        return Err(PublicationError::Limit);
    }
    result.map_err(|_| PublicationError::Serialization)?;
    Ok(output.bytes)
}
