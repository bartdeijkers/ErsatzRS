//! Block groups and blocks list — ports `Pages/Blocks.razor`.
use leptos::prelude::*;

use crate::admin::i18n::{delete_failed, scheduling_content_error_message, tr, TextKey};
use crate::admin::server_fns::blocks::{
    copy_block, create_block, create_block_group, delete_block, delete_block_group,
    format_duration_label, list_block_groups, list_blocks, BlockGroupRow, BlockRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    resource_state::{admin_loading_resource_view, admin_result_view},
    toast::{load_or_notify, use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/admin/pages/scheduling/blocks.module.css");

#[component]
pub fn BlocksPage() -> impl IntoView {
    let initial = Resource::new_blocking(
        || (),
        |()| async move { tokio::try_join!(list_block_groups(), list_blocks(String::new())) },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                initial.get().map(|result| admin_result_view(result, |(initial_groups, initial_blocks)| {
                    view! {
                        <BlocksIsland
                            initial_groups=initial_groups
                            initial_blocks=initial_blocks
                        />
                    }
                    .into_any()
                }))
            }}
        </Suspense>
    }
}

#[island]
fn BlocksIsland(
    initial_groups: Vec<BlockGroupRow>,
    initial_blocks: Vec<BlockRow>,
) -> impl IntoView {
    let toast = use_toast();
    // ── Groups ─────────────────────────────────────────────────────────────
    let current_groups = RwSignal::new(initial_groups);
    let groups: LocalResource<Vec<BlockGroupRow>> =
        LocalResource::new(move || async move { load_or_notify(list_block_groups().await, toast) });
    Effect::new(move |_| {
        if let Some(rows) = groups.get() {
            current_groups.set(rows);
        }
    });

    let new_group_name = RwSignal::new(String::new());
    let create_group_action = Action::new(|name: &String| create_block_group(name.clone()));

    // ── Blocks ─────────────────────────────────────────────────────────────
    let search = RwSignal::new(String::new());
    let current_blocks = RwSignal::new(initial_blocks);
    let blocks: LocalResource<Vec<BlockRow>> = LocalResource::new(move || {
        let q = search.get();
        async move { load_or_notify(list_blocks(q).await, toast) }
    });
    Effect::new(move |_| {
        if let Some(rows) = blocks.get() {
            current_blocks.set(rows);
        }
    });

    let new_block_name = RwSignal::new(String::new());
    let new_block_group_id: RwSignal<i64> = RwSignal::new(0);
    let create_block_action =
        Action::new(|(gid, name): &(i64, String)| create_block(*gid, name.clone()));

    // ── Copy block ─────────────────────────────────────────────────────────
    let copy_target: RwSignal<Option<BlockRow>> = RwSignal::new(None);
    let copy_open = RwSignal::new(false);
    let copy_name = RwSignal::new(String::new());
    let copy_group_id: RwSignal<i64> = RwSignal::new(0);
    let copy_action =
        Action::new(|(id, gid, name): &(i64, i64, String)| copy_block(*id, *gid, name.clone()));

    // ── Delete block group ─────────────────────────────────────────────────
    let del_group_target: RwSignal<Option<BlockGroupRow>> = RwSignal::new(None);
    let del_group_open = RwSignal::new(false);
    let del_group_action = Action::new(|id: &i64| delete_block_group(*id));

    // ── Delete block ───────────────────────────────────────────────────────
    let del_block_target: RwSignal<Option<BlockRow>> = RwSignal::new(None);
    let del_block_open = RwSignal::new(false);
    let del_block_action = Action::new(|id: &i64| delete_block(*id));

    Effect::new(move |_| {
        if let Some(res) = create_group_action.value().get() {
            match res {
                Ok(_) => {
                    toast.push(tr(TextKey::BlockGroupCreated), ToastSeverity::Success);
                    groups.refetch();
                    new_group_name.set(String::new());
                }
                Err(e) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::Error),
                        scheduling_content_error_message(e)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(res) = create_block_action.value().get() {
            match res {
                Ok(_) => {
                    toast.push(tr(TextKey::BlockCreated), ToastSeverity::Success);
                    blocks.refetch();
                    new_block_name.set(String::new());
                }
                Err(e) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::Error),
                        scheduling_content_error_message(e)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(res) = copy_action.value().get() {
            match res {
                Ok(id) => {
                    toast.push(tr(TextKey::BlockCopied), ToastSeverity::Success);
                    copy_open.set(false);
                    copy_target.set(None);
                    blocks.refetch();
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = window().location().set_href(&format!("/blocks/{id}"));
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = id;
                    }
                }
                Err(e) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::Error),
                        scheduling_content_error_message(e)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(res) = del_group_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push(tr(TextKey::BlockGroupDeleted), ToastSeverity::Success);
                    groups.refetch();
                    blocks.refetch();
                }
                Err(e) => toast.push(
                    delete_failed(scheduling_content_error_message(e)),
                    ToastSeverity::Error,
                ),
            }
            del_group_open.set(false);
        }
    });

    Effect::new(move |_| {
        if let Some(res) = del_block_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push(tr(TextKey::BlockDeleted), ToastSeverity::Success);
                    blocks.refetch();
                }
                Err(e) => toast.push(
                    delete_failed(scheduling_content_error_message(e)),
                    ToastSeverity::Error,
                ),
            }
            del_block_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <h1 class=style::page_title>{tr(TextKey::BlockGroups)}</h1>

            // ── Add block group ─────────────────────────────────────────
            <section class=style::section>
                <div class=style::add_row>
                    <div class=style::add_field>
                        <label class=style::add_label>{tr(TextKey::GroupName)}</label>
                        <input
                            class=style::add_input
                            placeholder=tr(TextKey::GroupName)
                            prop:value=move || new_group_name.get()
                            on:input=move |e| new_group_name.set(event_target_value(&e))
                        />
                    </div>
                    <button
                        class=style::btn_add
                        disabled=move || new_group_name.get().trim().is_empty()
                        on:click=move |_| {
                            let name = new_group_name.get().trim().to_string();
                            if !name.is_empty() {
                                create_group_action.dispatch(name);
                            }
                        }
                    >
                        {format!("+ {}", tr(TextKey::AddBlockGroup))}
                    </button>
                </div>
            </section>

            // ── Add block ───────────────────────────────────────────────
            <section class=style::section>
                <h2 class=style::page_title>{tr(TextKey::Blocks)}</h2>
                <div class=style::add_row>
                    <div class=style::add_field>
                        <label class=style::add_label>{tr(TextKey::BlockGroup)}</label>
                        <select
                            class=style::add_select
                            on:change=move |e| {
                                if let Ok(id) = event_target_value(&e).parse::<i64>() {
                                    new_block_group_id.set(id);
                                }
                            }
                        >
                            <option value="0">{tr(TextKey::SelectGroup)}</option>
                            {move || {
                                current_groups
                                    .get()
                                    .into_iter()
                                    .map(|g| {
                                        let id = g.id;
                                        let name = g.name.clone();
                                        view! { <option value=id.to_string()>{name}</option> }
                                    })
                                    .collect_view()
                            }}
                        </select>
                    </div>
                    <div class=style::add_field>
                        <label class=style::add_label>{tr(TextKey::BlockName)}</label>
                        <input
                            class=style::add_input
                            placeholder=tr(TextKey::BlockName)
                            prop:value=move || new_block_name.get()
                            on:input=move |e| new_block_name.set(event_target_value(&e))
                        />
                    </div>
                    <button
                        class=style::btn_add
                        disabled=move || {
                            new_block_group_id.get() == 0 || new_block_name.get().trim().is_empty()
                        }
                        on:click=move |_| {
                            let gid = new_block_group_id.get_untracked();
                            let name = new_block_name.get().trim().to_string();
                            if gid > 0 && !name.is_empty() {
                                create_block_action.dispatch((gid, name));
                            }
                        }
                    >
                        {format!("+ {}", tr(TextKey::AddBlock))}
                    </button>
                </div>
            </section>

            // ── Blocks table ────────────────────────────────────────────
            <div class=style::toolbar>
                <span class=style::search_icon aria-hidden="true"></span>
                <input
                    class=style::search
                    type="search"
                    placeholder=tr(TextKey::SearchForBlocks)
                    prop:value=move || search.get()
                    on:input=move |e| search.set(event_target_value(&e))
                />
            </div>

            {move || {
                let rows = current_blocks.get();
                if rows.is_empty() {
                    return view! { <div></div> }.into_any();
                }

                // Group rows by group_name for display
                let mut grouped: Vec<(String, Vec<BlockRow>)> = Vec::new();
                let mut current_group = String::new();
                for row in rows {
                    if row.group_name != current_group {
                        current_group.clone_from(&row.group_name);
                        grouped.push((current_group.clone(), Vec::new()));
                    }
                    if let Some(last) = grouped.last_mut() {
                        last.1.push(row);
                    }
                }
                // capture group_id of first block for group delete
                let del_group_rows: Vec<(String, i64, i64)> = grouped
                    .iter()
                    .map(|(name, rows)| {
                        (
                            name.clone(),
                            rows.first().map_or(0, |r| r.group_id),
                            rows.first().map_or(0, |r| r.id),
                        )
                    })
                    .collect();

                view! {
                    <div class=style::table_wrap>
                        <table>
                            <thead>
                                <tr>
                                    <th>{tr(TextKey::Name)}</th>
                                    <th>{tr(TextKey::Duration)}</th>
                                    <th>{tr(TextKey::StopScheduling)}</th>
                                    <th/>
                                </tr>
                            </thead>
                            <tbody>
                                {grouped.into_iter().zip(del_group_rows).map(|((gname, block_rows), (_, group_id, _))| {
                                    let gname_clone = gname.clone();
                                    view! {
                                        <tr>
                                            <td colspan="4" class=style::group_header>
                                                <div class=style::row_actions>
                                                    <span style="flex:1">{gname}</span>
                                                    <button
                                                        class=style::btn_delete
                                                        on:click=move |_| {
                                                            del_group_target.set(Some(BlockGroupRow {
                                                                id: group_id,
                                                                name: gname_clone.clone(),
                                                            }));
                                                            del_group_open.set(true);
                                                        }
                                                    >
                                                        {tr(TextKey::DeleteBlockGroup)}
                                                    </button>
                                                </div>
                                            </td>
                                        </tr>
                                        {block_rows.into_iter().filter(|b| b.id >= 0).map(|b| {
                                            let id = b.id;
                                            let name = b.name.clone();
                                            let dur = format_duration_label(b.minutes);
                                            let stop = stop_scheduling_label_i18n(b.stop_scheduling);
                                            let b_for_del = b.clone();
                                            let b_for_copy = b.clone();
                                            view! {
                                                <tr>
                                                    <td>
                                                        <a href=format!("/blocks/{id}")>{name.clone()}</a>
                                                    </td>
                                                    <td class=style::cell_muted>{dur}</td>
                                                    <td class=style::cell_muted>{stop}</td>
                                                    <td>
                                                        <div class=style::row_actions>
                                                            <a class=style::btn_edit href=format!("/blocks/{id}")>
                                                                {tr(TextKey::Edit)}
                                                            </a>
                                                            <button
                                                                class=style::btn_edit
                                                                on:click=move |_| {
                                                                    copy_name.set(format!("{} {}", name, tr(TextKey::CopyNameSuffix)));
                                                                    copy_group_id.set(b_for_copy.group_id);
                                                                    copy_target.set(Some(b_for_copy.clone()));
                                                                    copy_open.set(true);
                                                                }
                                                            >
                                                                {tr(TextKey::Copy)}
                                                            </button>
                                                            <button
                                                                class=style::btn_delete
                                                                on:click=move |_| {
                                                                    del_block_target.set(Some(b_for_del.clone()));
                                                                    del_block_open.set(true);
                                                                }
                                                            >
                                                                {tr(TextKey::Delete)}
                                                            </button>
                                                        </div>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }
                .into_any()
            }}

            // ── Delete dialogs — always rendered; visibility via `open` signal ──
            <DeleteDialog
                dialog_id="block-group-delete-dialog"
                open=del_group_open
                entity_name=Signal::derive(move || {
                    del_group_target.get().map(|grp| grp.name.clone()).unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if let Some(grp) = del_group_target.get_untracked() {
                        if confirmed { del_group_action.dispatch(grp.id); }
                        else { del_group_open.set(false); }
                    } else {
                        del_group_open.set(false);
                    }
                })
            />
            <DeleteDialog
                dialog_id="block-delete-dialog"
                open=del_block_open
                entity_name=Signal::derive(move || {
                    del_block_target.get().map(|b| b.name.clone()).unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if let Some(b) = del_block_target.get_untracked() {
                        if confirmed { del_block_action.dispatch(b.id); }
                        else { del_block_open.set(false); }
                    } else {
                        del_block_open.set(false);
                    }
                })
            />
            <div
                class=style::modal_backdrop
                style:display=move || if copy_open.get() { "" } else { "none" }
                on:click=move |_| copy_open.set(false)
            />
            <div
                class=style::modal_panel
                style:display=move || if copy_open.get() { "" } else { "none" }
            >
                <h2 class=style::section_title>{tr(TextKey::CopyBlock)}</h2>
                <label class=style::add_field>
                    <span class=style::add_label>{tr(TextKey::BlockGroup)}</span>
                    <select
                        class=style::add_select
                        prop:value=move || copy_group_id.get().to_string()
                        on:change=move |e| {
                            if let Ok(id) = event_target_value(&e).parse::<i64>() {
                                copy_group_id.set(id);
                            }
                        }
                    >
                        {move || {
                            current_groups
                                .get()
                                .into_iter()
                                .map(|g| {
                                    let id = g.id;
                                    let name = g.name.clone();
                                    view! { <option value=id.to_string()>{name}</option> }
                                })
                                .collect_view()
                        }}
                    </select>
                </label>
                <label class=style::add_field>
                    <span class=style::add_label>{tr(TextKey::BlockName)}</span>
                    <input
                        class=style::add_input
                        prop:value=move || copy_name.get()
                        on:input=move |e| copy_name.set(event_target_value(&e))
                    />
                </label>
                <div class=style::modal_actions>
                    <button class=style::btn_edit on:click=move |_| copy_open.set(false)>{tr(TextKey::Cancel)}</button>
                    <button
                        class=style::btn_add
                        disabled=move || {
                            copy_target.get().is_none()
                                || copy_group_id.get() == 0
                                || copy_name.get().trim().is_empty()
                        }
                        on:click=move |_| {
                            if let Some(target) = copy_target.get_untracked() {
                                copy_action.dispatch((
                                    target.id,
                                    copy_group_id.get_untracked(),
                                    copy_name.get_untracked().trim().to_string(),
                                ));
                            }
                        }
                    >
                        {tr(TextKey::CopyBlock)}
                    </button>
                </div>
            </div>
        </div>
    }
}

fn stop_scheduling_label_i18n(value: i32) -> &'static str {
    use crate::admin::server_fns::blocks::stop_scheduling;

    match value {
        stop_scheduling::BEFORE_DURATION_END => tr(TextKey::BeforeDurationEnd),
        stop_scheduling::AFTER_DURATION_END => tr(TextKey::AfterDurationEnd),
        _ => tr(TextKey::Unavailable),
    }
}
