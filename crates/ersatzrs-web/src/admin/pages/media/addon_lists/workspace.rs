//! ADR 0091 task layout, adapted from the locked admin-ux-organize reference.
//! Native forms carry unsaved input; application commands own persisted state.
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "ssr")]
pub mod actions;
pub mod import;
/// Not `ssr`-gated: the publication island must be compiled into the WASM
/// bundle for `hydrate_islands()` to attach it.
pub mod publication;
#[cfg(feature = "ssr")]
mod views;

stylance::import_crate_style!(
    style,
    "src/admin/pages/media/addon_lists/workspace.module.css"
);

pub const FORM: &str = "addon-list-workspace";

/// Browser-owned input, never a substitute for server authorization or revisions.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub list_id: i64,
    pub revision: i32,
    #[cfg(feature = "ssr")]
    #[serde(default)]
    pub playback_baseline: Option<ersatzrs_core::repositories::AddonMediaListPlaybackBaseline>,
    pub groups: BTreeMap<String, BTreeMap<String, String>>,
    pub selected: BTreeSet<i64>,
    pub query: String,
    pub category: String,
    #[serde(default)]
    pub sort: String,
    /// Derived from current authoritative readiness; never accepted from a POST.
    #[serde(skip)]
    pub publication_blocker_items: BTreeSet<i64>,
    pub page: usize,
    pub page_size: usize,
    #[serde(default)]
    pub undo: Option<BTreeMap<String, BTreeMap<String, String>>>,
    #[cfg(feature = "ssr")]
    #[serde(default)]
    pub saved_review_undo: Option<ersatzrs_core::repositories::AddonMediaListEditorSave>,
    #[serde(default)]
    pub confirmation: Option<String>,
    #[serde(default)]
    pub option_undo: Option<String>,
    #[serde(default)]
    pub expanded_bulk: bool,
    #[serde(default)]
    pub stale_groups: BTreeSet<String>,
}

#[derive(Clone, Default)]
pub struct Submission {
    #[cfg(feature = "ssr")]
    pub selection_preview: Option<ersatzrs_core::addon_list_selection::AddonListSelectionPreview>,
    pub draft: Draft,
    pub error: Option<String>,
    pub error_fields: Vec<String>,
    pub notice: Option<crate::admin::i18n::TextKey>,
    /// Error carried into a fresh page load by a redirect, such as an add whose
    /// list was created but whose settings were not saved.
    pub arrival_error: Option<crate::admin::i18n::TextKey>,
    /// This response produced a bulk proposal; its review takes focus so it is
    /// in view after the round trip. Later round trips keep the review in place.
    pub focus_bulk_preview: bool,
    pub replacement_searched: bool,
    pub replacement_results:
        Vec<crate::admin::server_fns::addon_media_lists::AddonMediaListReplacementRow>,
    pub comparison:
        Option<crate::admin::server_fns::addon_media_lists::AddonMediaListRevisionDiffRow>,
    #[cfg(feature = "ssr")]
    pub option_previews: Vec<ersatzrs_app::addon_media_list_options::AddonListOptionPreview>,
}

impl Draft {
    pub fn field(&self, group: &str, name: &str, saved: impl Into<String>) -> String {
        self.groups
            .get(group)
            .and_then(|fields| fields.get(name))
            .cloned()
            .unwrap_or_else(|| saved.into())
    }
}

/// Compact comparison token for native form input; never an authorization fence.
#[cfg(feature = "ssr")]
fn input_token(value: &str) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    Sha256::digest(value.replace("\r\n", "\n").as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing a string is infallible");
            output
        })
}

#[component]
pub fn AddonListWorkspacePage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    {
        use leptos_router::hooks::{use_location, use_params_map, use_query_map};
        let params = use_params_map();
        let id =
            params.with_untracked(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
        let item_id =
            params.with_untracked(|p| p.get("item_id").and_then(|s| s.parse::<i64>().ok()));
        let query = use_query_map();
        let path = use_location().pathname.get_untracked();
        let task = item_id.map_or_else(
            || path.rsplit('/').next().unwrap_or_default().to_owned(),
            |id| format!("item:{id}"),
        );
        let task = match task.as_str() {
            "settings" | "playback" | "activity" | "artwork" => task,
            _ if item_id.is_some() => task,
            _ => "items".to_owned(),
        };
        let item_id = if task == "artwork" {
            query.with_untracked(|q| {
                q.get("item_id")
                    .and_then(|s| s.parse::<i64>().ok())
                    .filter(|id| *id > 0)
            })
        } else {
            item_id
        };
        let task = if task == "artwork" {
            item_id.map_or(task.clone(), |id| format!("artwork:{id}"))
        } else {
            task
        };
        let task = if query.with_untracked(|q| q.get("disk_warning").is_some()) {
            "playback".to_owned()
        } else {
            task
        };
        let comparison = query.with_untracked(|q| {
            q.get("compare_left")
                .and_then(|s| s.parse().ok())
                .zip(q.get("compare_right").and_then(|s| s.parse().ok()))
        });
        super::admin_result_view(super::load_addon_media_list_editor_initial(id, comparison), move |initial| {
            let (form, items, readiness, history, metadata, providers, source, diff, impact, revisions, comparison, load_status) = initial;
            let Some(form) = form else {
                return view! { <p role="alert">{super::tr(super::TextKey::AddonMediaListNotFound)}</p> }.into_any();
            };
            let mut submission = use_context::<Submission>().unwrap_or_else(|| Submission {
                draft: Draft {
                    list_id: id, revision: form.draft_revision,
                    page_size: query.with_untracked(|q| q.get("page_size").and_then(|s| s.parse().ok()).filter(|n| [0,10,25,50,100].contains(n)).unwrap_or(10)),
                    page: query.with_untracked(|q| q.get("page").and_then(|s| s.parse::<usize>().ok()).unwrap_or(0)),
                    query: query.with_untracked(|q| q.get("q").unwrap_or_default()),
                    category: query.with_untracked(|q| q.get("review_category").unwrap_or_default()),
                    sort: query.with_untracked(|q| q.get("sort").filter(|s| valid_sort(s)).unwrap_or_default()),
                    ..Draft::default()
                },
                error: query.with_untracked(|q|q.get("disk_warning").filter(|s|s=="error").map(|_|super::tr(super::TextKey::AddonListDiskWarningFailed).to_owned())),
                arrival_error: query.with_untracked(|q|q.get("error").filter(|s|s=="save").map(|_|super::TextKey::AddonWorkspaceAddIncomplete)),
                ..Submission::default()
            });
            if submission.draft.list_id != id { submission = Submission { draft: Draft { list_id:id, revision:form.draft_revision, page_size:10, ..Draft::default() }, ..Submission::default() }; }
            if !submission.draft.groups.contains_key("playback") {
                let state = expect_context::<crate::host::state::AppState>();
                let baseline = tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(
                    state.application.ports().addon_media_lists().playback_baseline(id)));
                if let Ok((revision, baseline)) = baseline {
                    if revision == form.draft_revision {
                        submission.draft.playback_baseline = Some(baseline);
                    }
                }
            }
            if load_status.incomplete && submission.error.is_none() {
                submission.error = Some(super::tr(super::TextKey::AddonWorkspaceLoadIncomplete).to_owned());
            }
            submission.draft.publication_blocker_items = readiness.blockers.iter().filter_map(|blocker| blocker.item_id).collect();
            views::render(&views::Workspace { load_status, form, items, readiness, history, metadata, providers, source, diff, impact, revisions, comparison, submission, task, item_id })
        }).into_any()
    }
    #[cfg(not(feature = "ssr"))]
    {
        ().into_any()
    }
}

#[cfg(feature = "ssr")]
fn matches_row(
    d: &Draft,
    row: &super::AddonMediaListItemRow,
    presentation: super::AddonMediaListPresentationKind,
    diff: &super::AddonMediaListDraftDiffRow,
) -> bool {
    (d.query.is_empty()
        || row
            .display_title
            .to_lowercase()
            .contains(&d.query.to_lowercase()))
        && match d.category.as_str() {
            "" => true,
            "review" => super::item_needs_review(row, presentation),
            "blockers" => d.publication_blocker_items.contains(&row.id),
            "unknown" => row.availability == super::AddonMediaListItemAvailability::Unknown,
            "unnumbered" => row.season.is_none() || row.episode.is_none(),
            "excluded" => !row.included,
            "unavailable" => row.availability == super::AddonMediaListItemAvailability::Unavailable,
            "added" => diff.added_item_ids.contains(&row.id),
            "metadata_changed" => diff.metadata_changed_item_ids.contains(&row.id),
            "conflicted" => diff.conflicted_item_ids.contains(&row.id),
            "pending_removal" => diff.pending_removal_item_ids.contains(&row.id),
            "reappeared" => diff.reappeared_item_ids.contains(&row.id),
            "removal_candidate" => diff.removal_candidate_item_ids.contains(&row.id),
            _ => false,
        }
}

#[cfg(feature = "ssr")]
fn valid_sort(value: &str) -> bool {
    matches!(
        value,
        "" | "playlist_asc" | "playlist_desc" | "date_asc" | "date_desc" | "name_asc" | "name_desc"
    )
}

#[cfg(feature = "ssr")]
fn sorted_filtered<'a>(
    draft: &Draft,
    rows: &'a [super::AddonMediaListItemRow],
    presentation: super::AddonMediaListPresentationKind,
    diff: &super::AddonMediaListDraftDiffRow,
) -> Vec<&'a super::AddonMediaListItemRow> {
    let mut result: Vec<_> = rows
        .iter()
        .filter(|row| matches_row(draft, row, presentation, diff))
        .collect();
    result.sort_by(|a, b| {
        let order = match draft.sort.as_str() {
            "playlist_desc" => b.rank.cmp(&a.rank),
            "date_asc" | "date_desc" => {
                let dates = a.first_aired.is_none().cmp(&b.first_aired.is_none());
                if dates != std::cmp::Ordering::Equal {
                    dates
                } else if draft.sort == "date_desc" {
                    b.first_aired.cmp(&a.first_aired)
                } else {
                    a.first_aired.cmp(&b.first_aired)
                }
            }
            "name_asc" => a
                .display_title
                .to_lowercase()
                .cmp(&b.display_title.to_lowercase()),
            "name_desc" => b
                .display_title
                .to_lowercase()
                .cmp(&a.display_title.to_lowercase()),
            _ => a.rank.cmp(&b.rank),
        };
        order.then(a.rank.cmp(&b.rank)).then(a.id.cmp(&b.id))
    });
    result
}

#[cfg(all(test, feature = "ssr"))]
mod sort_tests {
    use super::*;

    #[test]
    fn review_sort_keeps_missing_dates_last_and_playlist_order_stable() {
        let rows = [
            super::super::AddonMediaListItemRow {
                id: 1,
                rank: 0,
                display_title: "Zulu".into(),
                first_aired: Some("2024-02-01".into()),
                ..Default::default()
            },
            super::super::AddonMediaListItemRow {
                id: 2,
                rank: 1,
                display_title: "alpha".into(),
                first_aired: Some("2023-01-01".into()),
                ..Default::default()
            },
            super::super::AddonMediaListItemRow {
                id: 3,
                rank: 2,
                display_title: "Undated".into(),
                ..Default::default()
            },
        ];
        let mut draft = Draft::default();
        let diff = super::super::AddonMediaListDraftDiffRow::default();
        for (sort, expected) in [
            ("playlist_asc", [1, 2, 3]),
            ("playlist_desc", [3, 2, 1]),
            ("date_asc", [2, 1, 3]),
            ("date_desc", [1, 2, 3]),
            ("name_asc", [2, 3, 1]),
            ("name_desc", [1, 3, 2]),
        ] {
            draft.sort = sort.into();
            let ids: Vec<_> = sorted_filtered(
                &draft,
                &rows,
                super::super::AddonMediaListPresentationKind::Auto,
                &diff,
            )
            .into_iter()
            .map(|row| row.id)
            .collect();
            assert_eq!(ids, expected, "{sort}");
        }
    }
}

#[cfg(feature = "ssr")]
fn review_snapshot(row: &super::AddonMediaListItemRow) -> String {
    serde_json::to_string(&ersatzrs_core::repositories::AddonMediaListItemReviewEdit {
        item_id: row.id,
        content_kind: row.content_kind,
        included: row.included,
        liveness_override: row.liveness_override,
        season: row.season,
        episode: row.episode,
    })
    .expect("review snapshot serializes")
}
