//! Block editor — ports `Pages/BlockEditor.razor`.
//!
//! The entire editor is a single `#[island]` so all item-list mutations
//! (add, remove, reorder, field changes) and the preview modal happen
//! client-side without round-trips. The `#[server]` functions are called only
//! for the initial data load (SSR) and for the save / preview actions.
#![allow(clippy::too_many_lines, dead_code)]
use std::fmt::Write;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{save_failed, scheduling_content_error_message, tr, TextKey};
use crate::admin::server_fns::blocks::{
    collection_type, get_block, get_block_items, load_block_editor_options, playback_order,
    preview_block_playout, save_block_with_items, stop_scheduling, BlockEditorOptions, BlockForm,
    BlockItemForm, BlockWithItemsForm, PlayoutPreviewRow,
};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/scheduling/block_editor.module.css");

// ---------------------------------------------------------------------------
// SSR wrapper
// ---------------------------------------------------------------------------

#[component]
pub fn BlockEditorPage() -> impl IntoView {
    let params = use_params_map();
    let block_id =
        move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));

    view! { <BlockEditorIsland block_id=block_id()/> }
}

// ---------------------------------------------------------------------------
// Island — owns the blocking resource; inner component is the fully hydrated editor
// ---------------------------------------------------------------------------

#[island]
fn BlockEditorIsland(block_id: i64) -> impl IntoView {
    let data: Resource<(Option<BlockForm>, Vec<BlockItemForm>, BlockEditorOptions)> =
        Resource::new_blocking(
            move || block_id,
            |id| async move {
                let block_fut = async move { get_block(id).await.unwrap_or(None) };
                let items_fut = async move { get_block_items(id).await.unwrap_or_default() };
                let options_fut =
                    async move { load_block_editor_options().await.unwrap_or_default() };
                let (block, items, options) = tokio::join!(block_fut, items_fut, options_fut);
                (block, items, options)
            },
        );

    view! {
        <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
            {move || {
                data.get().map(|(maybe_block, items, options)| match maybe_block {
                    Some(block) => view! {
                        <BlockEditorForm
                            initial_block=block
                            initial_items=items
                            options=options
                        />
                    }.into_any(),
                    None => view! {
                        <p class=style::empty>{tr(TextKey::BlockNotFound)}</p>
                    }.into_any(),
                })
            }}
        </Suspense>
    }
}

#[component]
fn BlockEditorForm(
    initial_block: BlockForm,
    initial_items: Vec<BlockItemForm>,
    options: BlockEditorOptions,
) -> impl IntoView {
    // ── Block metadata state ────────────────────────────────────────────
    let block_id = initial_block.id;
    let name = RwSignal::new(initial_block.name.clone());
    let group_name = initial_block.group_name.clone();
    let duration_hours = RwSignal::new(initial_block.minutes / 60);
    let duration_minutes = RwSignal::new(initial_block.minutes % 60);
    let stop_sched = RwSignal::new(initial_block.stop_scheduling);

    // ── Items state ─────────────────────────────────────────────────────
    let items: RwSignal<Vec<BlockItemForm>> = RwSignal::new(initial_items);

    // ── Selected item index (into items vec) ────────────────────────────
    let selected_idx: RwSignal<Option<usize>> = RwSignal::new(None);

    // ── Preview modal ────────────────────────────────────────────────────
    let preview_open = RwSignal::new(false);
    let preview_rows: RwSignal<Vec<PlayoutPreviewRow>> = RwSignal::new(Vec::new());
    let preview_pending = RwSignal::new(false);

    // ── Actions ─────────────────────────────────────────────────────────
    let save_action = Action::new(|form: &BlockWithItemsForm| {
        let form = form.clone();
        async move { save_block_with_items(form).await }
    });

    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(res) = save_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push(tr(TextKey::Saved), ToastSeverity::Success);
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = window().location().set_href("/blocks");
                    }
                }
                Err(e) => toast.push(
                    save_failed(scheduling_content_error_message(e)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    let on_save = move |()| {
        let minutes = duration_hours.get_untracked() * 60 + duration_minutes.get_untracked();
        let form = BlockWithItemsForm {
            id: block_id,
            group_id: initial_block.group_id,
            name: name.get_untracked().trim().to_string(),
            minutes,
            stop_scheduling: stop_sched.get_untracked(),
            items: items.get_untracked(),
        };
        save_action.dispatch(form);
    };

    let on_preview = move |()| {
        let minutes = duration_hours.get_untracked() * 60 + duration_minutes.get_untracked();
        let form = BlockWithItemsForm {
            id: block_id,
            group_id: initial_block.group_id,
            name: name.get_untracked().trim().to_string(),
            minutes,
            stop_scheduling: stop_sched.get_untracked(),
            items: items.get_untracked(),
        };
        preview_rows.set(Vec::new());
        preview_open.set(true);
        preview_pending.set(true);
        leptos::task::spawn_local(async move {
            let result = preview_block_playout(form).await.unwrap_or_default();
            preview_rows.set(result);
            preview_pending.set(false);
        });
    };

    // ── Item list operations ─────────────────────────────────────────────

    let add_item = move |()| {
        let next_idx = items
            .get_untracked()
            .iter()
            .map(|i| i.index)
            .max()
            .unwrap_or(-1)
            + 1;
        let new_item = BlockItemForm {
            id: 0,
            index: next_idx,
            collection_type: collection_type::COLLECTION,
            playback_order: playback_order::CHRONOLOGICAL,
            include_in_program_guide: true,
            ..Default::default()
        };
        items.update(|v| v.push(new_item));
        let new_sel = items.get_untracked().len().saturating_sub(1);
        selected_idx.set(Some(new_sel));
    };

    let remove_item = move |idx: usize| {
        items.update(|v| {
            v.remove(idx);
        });
        selected_idx.set(None);
    };

    let move_up = move |idx: usize| {
        items.update(|v| {
            if idx == 0 {
                return;
            }
            // Find the item with the next-lower index value
            let cur_idx_val = v[idx].index;
            if let Some(pos) = v
                .iter()
                .enumerate()
                .filter(|(_, i)| i.index < cur_idx_val)
                .max_by_key(|(_, i)| i.index)
                .map(|(p, _)| p)
            {
                v[idx].index = v[pos].index;
                v[pos].index = cur_idx_val;
                v.sort_by_key(|i| i.index);
            }
        });
    };

    view! {
        <div class=style::page>
            <noscript inner_html=block_editor_fallback_html(
                &initial_block,
                &items.get_untracked(),
                &options,
            )></noscript>
            // ── Sticky action bar ───────────────────────────────────────
            <div class=style::action_bar>
                <span class=style::action_bar_title>
                    {group_name.clone()} " / " {move || name.get()}
                </span>
                <button
                    class=style::btn_primary
                    disabled=move || save_action.pending().get()
                    on:click=move |_| on_save(())
                >
                    {tr(TextKey::SaveBlock)}
                </button>
                <button
                    class=style::btn_secondary
                    on:click=move |_| add_item(())
                >
                    {format!("+ {}", tr(TextKey::AddBlockItem))}
                </button>
                <button
                    class=style::btn_preview
                    on:click=move |_| on_preview(())
                >
                    {tr(TextKey::PreviewBlockPlayout)}
                </button>
            </div>

            <div class=style::content>
                // ── Block metadata ──────────────────────────────────────
                <div>
                    <h2 class=style::section_title>{tr(TextKey::Block)}</h2>
                    <div class=style::form_grid>
                        <div class=style::field>
                            <label class=style::label>{tr(TextKey::BlockGroup)}</label>
                            <input class=style::input disabled=true prop:value=group_name.clone()/>
                        </div>
                        <div class=style::field>
                            <label class=style::label>{tr(TextKey::BlockName)}</label>
                            <input
                                class=style::input
                                prop:value=move || name.get()
                                on:input=move |e| name.set(event_target_value(&e))
                            />
                        </div>
                        <div class=style::field>
                            <label class=style::label>{tr(TextKey::Duration)}</label>
                            <div class=style::duration_row>
                                <input
                                    class=style::duration_input
                                    type="number"
                                    min="0"
                                    prop:value=move || duration_hours.get().to_string()
                                    on:input=move |e| {
                                        if let Ok(h) = event_target_value(&e).parse::<i32>() {
                                            duration_hours.set(h.max(0));
                                        }
                                    }
                                />
                                <span class=style::duration_unit>{tr(TextKey::Hours)}</span>
                                <select
                                    class=style::select
                                    prop:value=move || duration_minutes.get().to_string()
                                    on:change=move |e| {
                                        if let Ok(m) = event_target_value(&e).parse::<i32>() {
                                            duration_minutes.set(m);
                                        }
                                    }
                                >
                                    {[0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55]
                                        .iter()
                                        .map(|&m| view! {
                                            <option value=m.to_string()>{m.to_string()}</option>
                                        })
                                        .collect_view()}
                                </select>
                                <span class=style::duration_unit>{tr(TextKey::Minutes)}</span>
                            </div>
                        </div>
                        <div class=style::field>
                            <label class=style::label>{tr(TextKey::StopScheduling)}</label>
                            <select
                                class=style::select
                                prop:value=move || stop_sched.get().to_string()
                                on:change=move |e| {
                                    if let Ok(v) = event_target_value(&e).parse::<i32>() {
                                        stop_sched.set(v);
                                    }
                                }
                            >
                                <option value=stop_scheduling::AFTER_DURATION_END.to_string()>
                                    {tr(TextKey::AfterDurationEnd)}
                                </option>
                                <option value=stop_scheduling::BEFORE_DURATION_END.to_string()>
                                    {tr(TextKey::BeforeDurationEnd)}
                                </option>
                            </select>
                        </div>
                    </div>
                </div>

                // ── Block items table ───────────────────────────────────
                <div>
                    <h2 class=style::section_title>{tr(TextKey::BlockItems)}</h2>
                    <div class=style::items_table_wrap>
                        <table class=style::items_table>
                            <thead>
                                <tr>
                                    <th>{tr(TextKey::Collection)}</th>
                                    <th>{tr(TextKey::PlaybackOrder)}</th>
                                    <th>{tr(TextKey::ShowInEpg)}</th>
                                    <th>{tr(TextKey::DisableWatermarks)}</th>
                                    <th/>
                                </tr>
                            </thead>
                            <tbody>
                                {move || {
                                    let current_items = items.get();
                                    current_items
                                        .into_iter()
                                        .enumerate()
                                        .map(|(idx, item)| {
                                            let is_selected = selected_idx.get() == Some(idx);
                                            let order_lbl = playback_order_label_i18n(
                                                item.playback_order,
                                                item.collection_type,
                                            );
                                            let col_name = item.collection_name.clone();
                                            view! {
                                                <tr
                                                    class=if is_selected { style::row_selected } else { "" }
                                                    on:click=move |_| {
                                                        selected_idx.set(Some(idx));
                                                    }
                                                >
                                                    <td>{col_name}</td>
                                                    <td>{order_lbl}</td>
                                                    <td>
                                                        <input
                                                            type="checkbox"
                                                            prop:checked=item.include_in_program_guide
                                                            on:change=move |e| {
                                                                let checked = event_target_checked(&e);
                                                                items.update(|v| {
                                                                    if let Some(i) = v.get_mut(idx) {
                                                                        i.include_in_program_guide = checked;
                                                                    }
                                                                });
                                                            }
                                                        />
                                                    </td>
                                                    <td>
                                                        <input
                                                            type="checkbox"
                                                            prop:checked=item.disable_watermarks
                                                            on:change=move |e| {
                                                                let checked = event_target_checked(&e);
                                                                items.update(|v| {
                                                                    if let Some(i) = v.get_mut(idx) {
                                                                        i.disable_watermarks = checked;
                                                                    }
                                                                });
                                                            }
                                                        />
                                                    </td>
                                                    <td>
                                                        <div class=style::row_actions>
                                                            <button
                                                                class=style::icon_btn
                                                                disabled=move || idx == 0
                                                                on:click=move |_| move_up(idx)
                                                                title=tr(TextKey::MoveUp)
                                                            >
                                                                "↑"
                                                            </button>
                                                            <button
                                                                class=style::icon_btn
                                                                disabled=move || { idx + 1 >= items.get().len() }
                                                                on:click=move |_| {
                                                                    items.update(|v| {
                                                                        let cur = v[idx].index;
                                                                        if let Some(pos) = v
                                                                            .iter()
                                                                            .enumerate()
                                                                            .filter(|(_, i)| i.index > cur)
                                                                            .min_by_key(|(_, i)| i.index)
                                                                            .map(|(p, _)| p)
                                                                        {
                                                                            v[idx].index = v[pos].index;
                                                                            v[pos].index = cur;
                                                                            v.sort_by_key(|i| i.index);
                                                                        }
                                                                    });
                                                                }
                                                                title=tr(TextKey::MoveDown)
                                                            >
                                                                "↓"
                                                            </button>
                                                            <button
                                                                class=format!("{} {}", style::icon_btn, style::icon_btn_delete)
                                                                on:click=move |_| remove_item(idx)
                                                                title=tr(TextKey::Remove)
                                                            >
                                                                "✕"
                                                            </button>
                                                        </div>
                                                    </td>
                                                </tr>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </tbody>
                        </table>
                    </div>
                </div>

                // ── Selected item editor ────────────────────────────────
                // Always rendered with a stable wrapper; content updates reactively.
                <div style:display=move || if selected_idx.get().is_some() { "" } else { "none" }>
                    {move || {
                        selected_idx.get().and_then(|idx| {
                            items.get().into_iter().nth(idx).map(|item| {
                                view! { <ItemEditor item=item idx=idx items=items options=options.clone()/> }
                            })
                        })
                    }}
                </div>
            </div>

            // ── Preview modal ───────────────────────────────────────────
            // Always rendered with stable DOM shape; visibility via style:display.
            <div
                class=style::modal_backdrop
                style:display=move || if preview_open.get() { "" } else { "none" }
                on:click=move |_| preview_open.set(false)
            />
            <div
                class=style::modal_panel
                style:display=move || if preview_open.get() { "" } else { "none" }
            >
                <button
                    class=style::modal_close
                    on:click=move |_| preview_open.set(false)
                >
                    {format!("x {}", tr(TextKey::Close))}
                </button>
                <h2 class=style::modal_title>{tr(TextKey::PreviewBlockPlayout)}</h2>
                <p
                    class=style::empty
                    style:display=move || if preview_pending.get() { "" } else { "none" }
                >
                    {tr(TextKey::ComputingPreview)}
                </p>
                <p
                    class=style::empty
                    style:display=move || {
                        if !preview_pending.get() && preview_rows.get().is_empty() { "" } else { "none" }
                    }
                >
                    {tr(TextKey::NoItemsToPreview)}
                </p>
                <table
                    class=style::preview_table
                    style:display=move || {
                        if !preview_pending.get() && !preview_rows.get().is_empty() { "" } else { "none" }
                    }
                >
                    <thead>
                        <tr>
                            <th>{tr(TextKey::Start)}</th>
                            <th>{tr(TextKey::Finish)}</th>
                            <th>{tr(TextKey::MediaItem)}</th>
                            <th>{tr(TextKey::Duration)}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || preview_rows.get().into_iter().map(|r| view! {
                            <tr>
                                <td>{r.start}</td>
                                <td>{r.finish}</td>
                                <td>{r.title}</td>
                                <td>{r.duration}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </div>
    }
}

fn playback_order_label_i18n(order: i32, collection_type: i32) -> &'static str {
    use crate::admin::server_fns::blocks::{collection_type as ct, playback_order as po};

    match order {
        po::CHRONOLOGICAL => tr(TextKey::Chronological),
        po::RANDOM => tr(TextKey::Random),
        po::SHUFFLE => tr(TextKey::Shuffle),
        po::SHUFFLE_IN_ORDER => tr(TextKey::ShuffleInOrder),
        po::SEASON_EPISODE => tr(TextKey::SeasonEpisode),
        po::RANDOM_ROTATION => tr(TextKey::RandomRotation),
        _ if collection_type == ct::SEARCH_QUERY => tr(TextKey::SearchQuery),
        _ => tr(TextKey::Unavailable),
    }
}

fn block_editor_fallback_html(
    block: &BlockForm,
    items: &[BlockItemForm],
    options: &BlockEditorOptions,
) -> String {
    let mut sorted_items = items.to_vec();
    sorted_items.sort_by_key(|item| item.index);
    let mut rows = String::new();
    if sorted_items.is_empty() {
        let _ = write!(
            rows,
            r#"<tr><td colspan="5" class="{}">{}</td></tr>"#,
            style::empty,
            escape_html(tr(TextKey::NoBlockItems))
        );
    } else {
        for (index, item) in sorted_items.iter().enumerate() {
            rows.push_str(&block_item_fallback_row_html(index, item, options));
        }
    }

    format!(
        r#"<div class="{action_bar}"><span class="{action_title}">{group} / {name}</span><button type="submit" form="block-items-fallback-form" class="{btn_primary}" name="action" value="save">{save_block}</button><button type="submit" form="block-items-fallback-form" class="{btn_secondary}" name="action" value="add">{add_block_item}</button><a href="/blocks" class="{btn_secondary}">{back}</a></div><section class="{content}"><form id="block-items-fallback-form" method="post" action="/api/blocks/items/save" class="{fallback_form}"><input type="hidden" name="block_id" value="{id}"><input type="hidden" name="group_id" value="{group_id}"><h2 class="{section_title}">{block_label}</h2><label class="{field}"><span class="{label}">{block_group_name}</span><input class="{input}" value="{group}" disabled></label><label class="{field}"><span class="{label}">{block_name}</span><input class="{input}" name="name" value="{name}"></label><label class="{field}"><span class="{label}">{duration}</span><span class="{duration_row}"><input class="{duration_input}" type="number" name="duration_hours" value="{hours}"><select class="{select}" name="duration_minutes">{minute_options}</select></span></label><label class="{field}"><span class="{label}">{stop_scheduling_block_items}</span>{stop_select}</label><h2 class="{section_title}">{block_items}</h2><div class="{items_table_wrap}"><table class="{items_table}"><thead><tr><th>{collection}</th><th>{target}</th><th>{playback}</th><th>{flags}</th><th></th></tr></thead><tbody>{rows}</tbody></table></div><h2 class="{section_title}">{add_block_item}</h2><label class="{field}"><span class="{label}">{collection}</span>{new_collection}</label></form></section>"#,
        action_bar = style::action_bar,
        action_title = style::action_bar_title,
        group = escape_html(&block.group_name),
        name = escape_html(&block.name),
        btn_primary = style::btn_primary,
        btn_secondary = style::btn_secondary,
        save_block = escape_html(tr(TextKey::SaveBlock)),
        add_block_item = escape_html(tr(TextKey::AddBlockItem)),
        back = escape_html(tr(TextKey::Back)),
        content = style::content,
        fallback_form = style::fallback_form,
        id = block.id,
        group_id = block.group_id,
        section_title = style::section_title,
        field = style::field,
        label = style::label,
        input = style::input,
        block_label = escape_html(tr(TextKey::Block)),
        block_group_name = escape_html(tr(TextKey::BlockGroupName)),
        block_name = escape_html(tr(TextKey::BlockName)),
        duration = escape_html(tr(TextKey::Duration)),
        stop_scheduling_block_items = escape_html(tr(TextKey::StopSchedulingBlockItems)),
        hours = block.minutes / 60,
        duration_row = style::duration_row,
        duration_input = style::duration_input,
        select = style::select,
        minute_options = minute_options_html(block.minutes % 60),
        stop_select = select_html(
            "stop_scheduling",
            block.stop_scheduling,
            &[
                (
                    stop_scheduling::BEFORE_DURATION_END,
                    tr(TextKey::BeforeDurationEnd)
                ),
                (
                    stop_scheduling::AFTER_DURATION_END,
                    tr(TextKey::AfterDurationEnd)
                ),
            ],
        ),
        block_items = escape_html(tr(TextKey::BlockItems)),
        items_table_wrap = style::items_table_wrap,
        items_table = style::items_table,
        collection = escape_html(tr(TextKey::Collection)),
        target = escape_html(tr(TextKey::Target)),
        playback = escape_html(tr(TextKey::Playback)),
        flags = escape_html(tr(TextKey::Flags)),
        rows = rows,
        new_collection = option_select_html("new_collection_id", None, &options.collections),
    )
}

fn block_item_fallback_row_html(
    index: usize,
    item: &BlockItemForm,
    options: &BlockEditorOptions,
) -> String {
    format!(
        r#"<tr><td><input type="hidden" name="item_index" value="{item_index}"><input type="hidden" name="item_id" value="{id}">{collection_type}</td><td>{collection}{multi_collection}{smart_collection}{media_item}<input class="{input}" name="item_search_title" placeholder="{search_title_label}" value="{search_title}"><input class="{input}" name="item_search_query" placeholder="{search_query_label}" value="{search_query}"><input type="hidden" name="item_collection_name" value="{collection_name}"></td><td>{playback}</td><td><label class="{check}"><input type="checkbox" name="item_include_in_program_guide_index" value="{item_index}" {epg_checked}>{show_in_epg}</label><label class="{check}"><input type="checkbox" name="item_disable_watermarks_index" value="{item_index}" {watermark_checked}>{disable_watermarks}</label><input class="{input}" name="item_watermark_ids" placeholder="{watermark_ids_label}" value="{watermark_ids}"><input class="{input}" name="item_graphics_element_ids" placeholder="{graphics_ids_label}" value="{graphics_ids}"></td><td><div class="{row_actions}"><button class="{icon_btn}" type="submit" name="action" value="copy:{index}">{copy}</button><button class="{icon_btn}" type="submit" name="action" value="move-up:{index}" {move_up_disabled}>{up}</button><button class="{icon_btn}" type="submit" name="action" value="move-down:{index}">{down}</button><button class="{icon_btn}" type="submit" name="action" value="delete:{index}">{delete}</button></div></td></tr>"#,
        item_index = item.index,
        id = item.id,
        collection_type = select_html(
            "item_collection_type",
            item.collection_type,
            &[
                (collection_type::COLLECTION, tr(TextKey::Collection)),
                (
                    collection_type::TELEVISION_SHOW,
                    tr(TextKey::TelevisionShow)
                ),
                (
                    collection_type::TELEVISION_SEASON,
                    tr(TextKey::TelevisionSeason)
                ),
                (
                    collection_type::MULTI_COLLECTION,
                    tr(TextKey::MultiCollection)
                ),
                (
                    collection_type::SMART_COLLECTION,
                    tr(TextKey::SmartCollection)
                ),
                (collection_type::SEARCH_QUERY, tr(TextKey::SearchQuery)),
            ],
        ),
        collection = option_select_html(
            "item_collection_id",
            item.collection_id,
            &options.collections
        ),
        multi_collection = option_select_html(
            "item_multi_collection_id",
            item.multi_collection_id,
            &options.multi_collections
        ),
        smart_collection = option_select_html(
            "item_smart_collection_id",
            item.smart_collection_id,
            &options.smart_collections
        ),
        media_item = media_item_select_html(item, options),
        input = style::input,
        search_title = escape_html(&item.search_title.clone().unwrap_or_default()),
        search_query = escape_html(&item.search_query.clone().unwrap_or_default()),
        search_title_label = escape_html(tr(TextKey::SearchTitle)),
        search_query_label = escape_html(tr(TextKey::SearchQuery)),
        collection_name = escape_html(&item.collection_name),
        playback = select_html(
            "item_playback_order",
            item.playback_order,
            &[
                (playback_order::CHRONOLOGICAL, tr(TextKey::Chronological)),
                (playback_order::RANDOM, tr(TextKey::Random)),
                (playback_order::SHUFFLE, tr(TextKey::Shuffle)),
                (
                    playback_order::SHUFFLE_IN_ORDER,
                    tr(TextKey::ShuffleInOrder)
                ),
                (playback_order::SEASON_EPISODE, tr(TextKey::SeasonEpisode)),
                (playback_order::RANDOM_ROTATION, tr(TextKey::RandomRotation)),
            ],
        ),
        check = style::check,
        show_in_epg = escape_html(tr(TextKey::ShowInEpg)),
        disable_watermarks = escape_html(tr(TextKey::DisableWatermarks)),
        epg_checked = if item.include_in_program_guide {
            "checked"
        } else {
            ""
        },
        watermark_checked = if item.disable_watermarks {
            "checked"
        } else {
            ""
        },
        watermark_ids = id_csv(&item.watermark_ids),
        graphics_ids = id_csv(&item.graphics_element_ids),
        watermark_ids_label = escape_html(tr(TextKey::WatermarkIds)),
        graphics_ids_label = escape_html(tr(TextKey::GraphicsIds)),
        row_actions = style::row_actions,
        icon_btn = style::icon_btn,
        index = index,
        copy = escape_html(tr(TextKey::Copy)),
        up = escape_html(tr(TextKey::MoveUp)),
        down = escape_html(tr(TextKey::MoveDown)),
        delete = escape_html(tr(TextKey::Delete)),
        move_up_disabled = if index == 0 { "disabled" } else { "" },
    )
}

fn media_item_select_html(item: &BlockItemForm, options: &BlockEditorOptions) -> String {
    let source = if item.collection_type == collection_type::TELEVISION_SEASON {
        &options.seasons
    } else {
        &options.shows
    };
    option_select_html("item_media_item_id", item.media_item_id, source)
}

fn minute_options_html(selected: i32) -> String {
    [0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55]
        .into_iter()
        .fold(String::new(), |mut html, value| {
            let selected_attr = if value == selected { " selected" } else { "" };
            let _ = write!(
                html,
                r#"<option value="{value}"{selected_attr}>{value}</option>"#
            );
            html
        })
}

fn select_html(name: &str, selected: i32, options: &[(i32, &'static str)]) -> String {
    let options = options
        .iter()
        .fold(String::new(), |mut html, (value, label)| {
            let selected_attr = if *value == selected { " selected" } else { "" };
            let _ = write!(
                html,
                r#"<option value="{}"{}>{}</option>"#,
                value,
                selected_attr,
                escape_html(label)
            );
            html
        });
    format!(
        r#"<select class="{}" name="{}">{}</select>"#,
        style::select,
        escape_html(name),
        options
    )
}

fn option_select_html(
    name: &str,
    selected: Option<i64>,
    options: &[crate::admin::server_fns::blocks::BlockEditorOption],
) -> String {
    let mut option_html = r#"<option value=""></option>"#.to_string();
    for option in options {
        let selected_attr = if Some(option.id) == selected {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            option_html,
            r#"<option value="{}"{}>{}</option>"#,
            option.id,
            selected_attr,
            escape_html(&option.name)
        );
    }
    format!(
        r#"<select class="{}" name="{}">{}</select>"#,
        style::select,
        escape_html(name),
        option_html
    )
}

fn id_csv(ids: &[i64]) -> String {
    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// Item editor sub-component (inside the island, not a separate island)
// ---------------------------------------------------------------------------

#[component]
fn ItemEditor(
    item: BlockItemForm,
    idx: usize,
    items: RwSignal<Vec<BlockItemForm>>,
    options: BlockEditorOptions,
) -> impl IntoView {
    use crate::admin::server_fns::blocks::{
        collection_type, playback_order, search_collections_for_block,
        search_multi_collections_for_block, search_seasons_for_block, search_shows_for_block,
        search_smart_collections_for_block,
    };

    let col_type = RwSignal::new(item.collection_type);
    let col_query = RwSignal::new(item.collection_name.clone());
    let suggestions: RwSignal<Vec<(i64, String)>> = RwSignal::new(Vec::new());
    let show_suggest = RwSignal::new(false);
    let pb_order = RwSignal::new(item.playback_order);
    let search_title = RwSignal::new(item.search_title.clone().unwrap_or_default());
    let search_query_val = RwSignal::new(item.search_query.clone().unwrap_or_default());

    // Sync local col_type → items list; skip the initial run so loaded
    // item references are not wiped when the editor first mounts.
    Effect::new(move |prev: Option<()>| {
        let ct = col_type.get();
        if prev.is_none() {
            // First run: signal is already in sync with the loaded item.
            return;
        }
        items.update(|v| {
            if let Some(i) = v.get_mut(idx) {
                i.collection_type = ct;
                i.collection_id = None;
                i.multi_collection_id = None;
                i.smart_collection_id = None;
                i.media_item_id = None;
                i.collection_name = String::new();
            }
        });
        col_query.set(String::new());
        suggestions.set(Vec::new());
        show_suggest.set(false);
    });

    // Sync pb_order → items
    Effect::new(move |prev: Option<()>| {
        if prev.is_none() {
            return;
        }
        let order = pb_order.get();
        items.update(|v| {
            if let Some(i) = v.get_mut(idx) {
                i.playback_order = order;
            }
        });
    });

    // Sync search fields → items
    Effect::new(move |prev: Option<()>| {
        if prev.is_none() {
            return;
        }
        let title = search_title.get();
        let query = search_query_val.get();
        items.update(|v| {
            if let Some(i) = v.get_mut(idx) {
                i.search_title = if title.is_empty() { None } else { Some(title) };
                i.search_query = if query.is_empty() { None } else { Some(query) };
                i.collection_name = i
                    .search_title
                    .clone()
                    .or_else(|| i.search_query.clone())
                    .unwrap_or_default();
            }
        });
    });

    // Fetch suggestions when col_query changes
    Effect::new(move |_| {
        let q = col_query.get();
        let ct = col_type.get_untracked();
        let trimmed = q.trim().to_string();
        if trimmed.is_empty() {
            suggestions.set(Vec::new());
            show_suggest.set(false);
            return;
        }
        show_suggest.set(true);
        leptos::task::spawn_local(async move {
            let results: Vec<(i64, String)> = match ct {
                t if t == collection_type::COLLECTION => search_collections_for_block(trimmed)
                    .await
                    .unwrap_or_default(),
                t if t == collection_type::SMART_COLLECTION => {
                    search_smart_collections_for_block(trimmed)
                        .await
                        .unwrap_or_default()
                }
                t if t == collection_type::MULTI_COLLECTION => {
                    search_multi_collections_for_block(trimmed)
                        .await
                        .unwrap_or_default()
                }
                t if t == collection_type::TELEVISION_SHOW => {
                    search_shows_for_block(trimmed).await.unwrap_or_default()
                }
                t if t == collection_type::TELEVISION_SEASON => {
                    search_seasons_for_block(trimmed).await.unwrap_or_default()
                }
                _ => vec![],
            };
            suggestions.set(results);
        });
    });

    let select_suggestion = move |id: i64, name: String| {
        let ct = col_type.get_untracked();
        items.update(|v| {
            if let Some(item) = v.get_mut(idx) {
                item.collection_name.clone_from(&name);
                match ct {
                    t if t == collection_type::COLLECTION => {
                        item.collection_id = Some(id);
                    }
                    t if t == collection_type::SMART_COLLECTION => {
                        item.smart_collection_id = Some(id);
                    }
                    t if t == collection_type::MULTI_COLLECTION => {
                        item.multi_collection_id = Some(id);
                    }
                    t if t == collection_type::TELEVISION_SHOW
                        || t == collection_type::TELEVISION_SEASON =>
                    {
                        item.media_item_id = Some(id);
                    }
                    _ => {}
                }
            }
        });
        col_query.set(name);
        suggestions.set(Vec::new());
        show_suggest.set(false);
    };

    let playback_order_options = move || {
        let ct = col_type.get();
        match ct {
            t if t == collection_type::MULTI_COLLECTION => vec![
                (playback_order::SHUFFLE, tr(TextKey::Shuffle)),
                (
                    playback_order::SHUFFLE_IN_ORDER,
                    tr(TextKey::ShuffleInOrder),
                ),
            ],
            t if t == collection_type::TELEVISION_SHOW => vec![
                (playback_order::SEASON_EPISODE, tr(TextKey::SeasonEpisode)),
                (playback_order::CHRONOLOGICAL, tr(TextKey::Chronological)),
                (playback_order::SHUFFLE, tr(TextKey::Shuffle)),
                (playback_order::RANDOM, tr(TextKey::Random)),
            ],
            t if t == collection_type::COLLECTION || t == collection_type::SMART_COLLECTION => {
                vec![
                    (playback_order::CHRONOLOGICAL, tr(TextKey::Chronological)),
                    (playback_order::SHUFFLE, tr(TextKey::Shuffle)),
                    (playback_order::RANDOM, tr(TextKey::Random)),
                    (playback_order::RANDOM_ROTATION, tr(TextKey::RandomRotation)),
                ]
            }
            _ => vec![
                (playback_order::CHRONOLOGICAL, tr(TextKey::Chronological)),
                (playback_order::SHUFFLE, tr(TextKey::Shuffle)),
                (playback_order::RANDOM, tr(TextKey::Random)),
            ],
        }
    };

    let needs_autocomplete = move || {
        let ct = col_type.get();
        ct == collection_type::COLLECTION
            || ct == collection_type::SMART_COLLECTION
            || ct == collection_type::MULTI_COLLECTION
            || ct == collection_type::TELEVISION_SHOW
            || ct == collection_type::TELEVISION_SEASON
    };

    view! {
        <div class=style::item_editor>
            <h3 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-200);margin:0">
                {tr(TextKey::BlockItem)}
            </h3>

            <div class=style::field>
                <label class=style::label>{tr(TextKey::CollectionType)}</label>
                <select
                    class=style::select
                    prop:value=move || col_type.get().to_string()
                    on:change=move |e| {
                        if let Ok(v) = event_target_value(&e).parse::<i32>() {
                            col_type.set(v);
                        }
                    }
                >
                    <option value=collection_type::COLLECTION.to_string()>{tr(TextKey::Collection)}</option>
                    <option value=collection_type::TELEVISION_SHOW.to_string()>{tr(TextKey::TelevisionShow)}</option>
                    <option value=collection_type::TELEVISION_SEASON.to_string()>{tr(TextKey::TelevisionSeason)}</option>
                    <option value=collection_type::MULTI_COLLECTION.to_string()>{tr(TextKey::MultiCollection)}</option>
                    <option value=collection_type::SMART_COLLECTION.to_string()>{tr(TextKey::SmartCollection)}</option>
                    <option value=collection_type::SEARCH_QUERY.to_string()>{tr(TextKey::SearchQuery)}</option>
                </select>
            </div>

            // Collection autocomplete — always rendered; hidden for SEARCH_QUERY type.
            <div
                class=style::field
                style:display=move || if needs_autocomplete() { "" } else { "none" }
            >
                <label class=style::label>{tr(TextKey::Collection)}</label>
                <div class=style::autocomplete>
                    <input
                        class=style::autocomplete_input
                        type="text"
                        placeholder=tr(TextKey::TypeToSearch)
                        prop:value=move || col_query.get()
                        disabled=move || !needs_autocomplete()
                        on:input=move |e| col_query.set(event_target_value(&e))
                        on:blur=move |_| {
                            // small delay so mousedown on list item fires first
                            leptos::task::spawn_local(async move {
                                gloo_timers::future::TimeoutFuture::new(150).await;
                                show_suggest.set(false);
                            });
                        }
                        on:focus=move |_| {
                            if !col_query.get().trim().is_empty() {
                                show_suggest.set(true);
                            }
                        }
                    />
                    <ul
                        class=style::autocomplete_list
                        style:display=move || {
                            if show_suggest.get() && !suggestions.get().is_empty() { "" } else { "none" }
                        }
                    >
                        {move || suggestions.get().into_iter().map(|(id, name)| {
                            let name_clone = name.clone();
                            view! {
                                <li
                                    class=style::autocomplete_item
                                    on:mousedown=move |e| {
                                        e.prevent_default();
                                        select_suggestion(id, name_clone.clone());
                                    }
                                >
                                    {name}
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                </div>
            </div>

            // Search fields — always rendered; hidden for non-SEARCH_QUERY types.
            <div
                class=style::field
                style:display=move || {
                    if col_type.get() == collection_type::SEARCH_QUERY { "" } else { "none" }
                }
            >
                <label class=style::label>{tr(TextKey::SearchTitleDisplay)}</label>
                <input
                    class=style::input
                    placeholder=tr(TextKey::OptionalDisplayTitle)
                    disabled=move || col_type.get() != collection_type::SEARCH_QUERY
                    prop:value=move || search_title.get()
                    on:input=move |e| search_title.set(event_target_value(&e))
                />
            </div>
            <div
                class=style::field
                style:display=move || {
                    if col_type.get() == collection_type::SEARCH_QUERY { "" } else { "none" }
                }
            >
                <label class=style::label>{tr(TextKey::SearchQuery)}</label>
                <input
                    class=style::input
                    placeholder=tr(TextKey::BlockSearchQueryPlaceholder)
                    disabled=move || col_type.get() != collection_type::SEARCH_QUERY
                    prop:value=move || search_query_val.get()
                    on:input=move |e| search_query_val.set(event_target_value(&e))
                />
            </div>

            <div class=style::field>
                <label class=style::label>{tr(TextKey::PlaybackOrder)}</label>
                <select
                    class=style::select
                    prop:value=move || pb_order.get().to_string()
                    on:change=move |e| {
                        if let Ok(v) = event_target_value(&e).parse::<i32>() {
                            pb_order.set(v);
                        }
                    }
                >
                    {move || playback_order_options().into_iter().map(|(v, lbl)| {
                        view! { <option value=v.to_string()>{lbl}</option> }
                    }).collect_view()}
                </select>
            </div>
            {multi_select_i64(tr(TextKey::Watermarks), &item.watermark_ids, options.watermarks.clone(), move |values| {
                update_item(items, idx, |item| item.watermark_ids = values);
            })}
            {multi_select_i64(tr(TextKey::GraphicsElements), &item.graphics_element_ids, options.graphics_elements.clone(), move |values| {
                update_item(items, idx, |item| item.graphics_element_ids = values);
            })}
        </div>
    }
}

fn update_item<F>(items: RwSignal<Vec<BlockItemForm>>, idx: usize, update: F)
where
    F: FnOnce(&mut BlockItemForm),
{
    items.update(|values| {
        if let Some(item) = values.get_mut(idx) {
            update(item);
        }
    });
}

fn multi_select_i64<F>(
    label: &'static str,
    selected: &[i64],
    options: Vec<crate::admin::server_fns::blocks::BlockEditorOption>,
    on_change: F,
) -> AnyView
where
    F: Fn(Vec<i64>) + Copy + 'static,
{
    let selected = selected.to_vec();
    view! {
        <label class=style::field>
            <span class=style::label>{label}</span>
            <span class=style::check_list>
                {options.into_iter().map(|option| {
                    let selected_values = selected.clone();
                    let is_selected = selected_values.contains(&option.id);
                    let option_id = option.id;
                    view! {
                        <label class=style::check>
                            <input
                                type="checkbox"
                                value=option.id.to_string()
                                checked=is_selected
                                prop:checked=is_selected
                                on:change=move |event| {
                                    let mut values = selected_values.clone();
                                    if event_target_checked(&event) {
                                        if !values.contains(&option_id) {
                                            values.push(option_id);
                                        }
                                    } else {
                                        values.retain(|value| *value != option_id);
                                    }
                                    on_change(values);
                                }
                            />
                            <span>{option.name}</span>
                        </label>
                    }
                }).collect_view()}
            </span>
        </label>
    }
    .into_any()
}
