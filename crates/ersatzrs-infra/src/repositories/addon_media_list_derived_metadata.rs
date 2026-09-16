//! Bounded provider-neutral parent metadata fallback (ADR 0094).
use std::collections::BTreeSet;

use ersatzrs_core::repositories::{AddonMediaListNfoMetadata, Result};
use serde_json::Value;
use sqlx::Row;

use super::{addon_media_list::metadata_form_from_json, SqlxResultExt as _};
use crate::db::DbPool;

const MAX_VALUES: usize = 1024;
const MAX_DERIVED_BYTES: usize = 64 * 1024;

struct Derived {
    values: [Vec<String>; 4],
    seen: [BTreeSet<String>; 4],
    age: Option<u32>,
    bytes: usize,
    limited: bool,
}

impl Default for Derived {
    fn default() -> Self {
        Self {
            values: std::array::from_fn(|_| Vec::new()),
            seen: std::array::from_fn(|_| BTreeSet::new()),
            age: None,
            bytes: 0,
            limited: false,
        }
    }
}

impl Derived {
    fn add(&mut self, metadata: &AddonMediaListNfoMetadata) {
        for (index, values) in [
            &metadata.genres,
            &metadata.tags,
            &metadata.languages,
            &metadata.studios,
        ]
        .into_iter()
        .enumerate()
        {
            for value in values {
                let value = value.trim();
                if value.is_empty() || self.seen[index].contains(&value.to_lowercase()) {
                    continue;
                }
                if self.values[index].len() == MAX_VALUES
                    || self.bytes + value.len() > MAX_DERIVED_BYTES
                {
                    self.limited = true;
                    continue;
                }
                self.bytes += value.len();
                self.seen[index].insert(value.to_lowercase());
                self.values[index].push(value.to_owned());
            }
        }
        for rating in &metadata.content_ratings {
            if let Some(age) = rating
                .strip_prefix("age:")
                .and_then(|age| age.parse::<u32>().ok())
            {
                self.age = Some(self.age.map_or(age, |previous| previous.max(age)));
            }
        }
    }

    fn apply(self, parent: &mut AddonMediaListNfoMetadata, overrides: &Value) {
        for ((name, target), values) in [
            ("genres", &mut parent.genres),
            ("tags", &mut parent.tags),
            ("languages", &mut parent.languages),
            ("studios", &mut parent.studios),
        ]
        .into_iter()
        .zip(self.values)
        {
            if target.is_empty() && overrides.get(name).is_none() {
                *target = values;
            }
        }
        if parent.content_ratings.is_empty() && overrides.get("content_ratings").is_none() {
            if let Some(age) = self.age {
                parent.content_ratings.push(format!("age:{age}"));
            }
        }
    }
}

pub(super) async fn fill_parent(
    pool: &DbPool,
    list_id: i64,
    parent: &mut AddonMediaListNfoMetadata,
    overrides: &Value,
) -> Result<()> {
    let mut derived = Derived::default();
    let mut offset = 0_i64;
    loop {
        let rows = sqlx::query(
            r#"SELECT "ProviderMetadataJson", "MetadataOverrideJson" FROM "AddonMediaListItem"
            WHERE "AddonMediaListId" = ? AND "Included" = 1 AND "ParentProviderId" IS NULL
            ORDER BY "Rank", "Id" LIMIT 100 OFFSET ?"#,
        )
        .bind(list_id)
        .bind(offset)
        .fetch_all(pool)
        .await
        .repository()?;
        for row in &rows {
            let provider: String = row.try_get("ProviderMetadataJson").repository()?;
            let overrides: String = row.try_get("MetadataOverrideJson").repository()?;
            derived.add(&metadata_form_from_json(&provider, &overrides, None)?.metadata);
        }
        if rows.len() < 100 {
            break;
        }
        offset += 100;
    }
    sqlx::query(r#"UPDATE "AddonMediaList" SET "DerivedMetadataLimited"=? WHERE "Id"=?"#)
        .bind(derived.limited)
        .bind(list_id)
        .execute(pool)
        .await
        .repository()?;
    if derived.limited {
        tracing::warn!(
            addon_media_list_id = list_id,
            "Derived show metadata reached the bounded aggregation limit"
        );
    }
    derived.apply(parent, overrides);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_stable_union_and_source_age_without_overwriting_explicit_values() {
        let mut derived = Derived::default();
        for (tag, age) in [("Archive", "age:0"), ("archive", "age:18")] {
            derived.add(&AddonMediaListNfoMetadata {
                tags: vec![tag.into()],
                genres: vec!["Documentary".into()],
                languages: vec!["en".into()],
                studios: vec!["Channel".into()],
                content_ratings: vec![age.into(), "fixture:PG".into()],
                ..Default::default()
            });
        }
        let mut parent = AddonMediaListNfoMetadata {
            genres: vec!["Header".into()],
            ..Default::default()
        };
        derived.apply(&mut parent, &serde_json::json!({"languages": []}));
        assert_eq!(parent.tags, ["Archive"]);
        assert_eq!(parent.genres, ["Header"]);
        assert_eq!(parent.studios, ["Channel"]);
        assert!(parent.languages.is_empty());
        assert_eq!(parent.content_ratings, ["age:18"]);
    }

    #[test]
    fn bounded_aggregation_retains_known_values_and_does_not_invent_ratings() {
        let mut derived = Derived::default();
        derived.add(&AddonMediaListNfoMetadata {
            tags: (0..1100).map(|index| format!("tag-{index}")).collect(),
            content_ratings: vec!["fictional:PG".into()],
            ..Default::default()
        });
        assert!(derived.limited);
        let mut parent = AddonMediaListNfoMetadata::default();
        derived.apply(&mut parent, &serde_json::json!({}));
        assert_eq!(parent.tags.len(), MAX_VALUES);
        assert!(parent.content_ratings.is_empty());
    }
}
