//! Block groups and blocks list — ports `Pages/Blocks.razor`.
#![allow(clippy::too_many_lines, dead_code)]
use leptos::prelude::*;

use crate::server::blocks::{
    create_block, create_block_group, delete_block, delete_block_group, format_duration_label,
    list_block_groups, list_blocks, BlockGroupRow, BlockRow,
};
use crate::ui::primitives::{
    dialog::DeleteDialog,
    toast::{use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/pages/scheduling/blocks.module.css");

#[component]
pub fn BlocksPage() -> impl IntoView {
    // ── Groups ─────────────────────────────────────────────────────────────
    let groups: Resource<Vec<BlockGroupRow>> = Resource::new(
        || (),
        |()| async { list_block_groups().await.unwrap_or_default() },
    );

    let new_group_name = RwSignal::new(String::new());
    let create_group_action = Action::new(|name: &String| create_block_group(name.clone()));

    // ── Blocks ─────────────────────────────────────────────────────────────
    let search = RwSignal::new(String::new());
    let blocks: Resource<Vec<BlockRow>> = Resource::new(
        move || search.get(),
        |q| async move { list_blocks(q).await.unwrap_or_default() },
    );

    let new_block_name = RwSignal::new(String::new());
    let new_block_group_id: RwSignal<i64> = RwSignal::new(0);
    let create_block_action =
        Action::new(|(gid, name): &(i64, String)| create_block(*gid, name.clone()));

    // ── Delete block group ─────────────────────────────────────────────────
    let del_group_target: RwSignal<Option<BlockGroupRow>> = RwSignal::new(None);
    let del_group_open = RwSignal::new(false);
    let del_group_action = Action::new(|id: &i64| delete_block_group(*id));

    // ── Delete block ───────────────────────────────────────────────────────
    let del_block_target: RwSignal<Option<BlockRow>> = RwSignal::new(None);
    let del_block_open = RwSignal::new(false);
    let del_block_action = Action::new(|id: &i64| delete_block(*id));

    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(res) = create_group_action.value().get() {
            match res {
                Ok(_) => {
                    toast.push("Block group created.", ToastSeverity::Success);
                    groups.refetch();
                    new_group_name.set(String::new());
                }
                Err(e) => toast.push(format!("Error: {e}"), ToastSeverity::Error),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(res) = create_block_action.value().get() {
            match res {
                Ok(_) => {
                    toast.push("Block created.", ToastSeverity::Success);
                    blocks.refetch();
                    new_block_name.set(String::new());
                }
                Err(e) => toast.push(format!("Error: {e}"), ToastSeverity::Error),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(res) = del_group_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push("Block group deleted.", ToastSeverity::Success);
                    groups.refetch();
                    blocks.refetch();
                }
                Err(e) => toast.push(format!("Error: {e}"), ToastSeverity::Error),
            }
            del_group_open.set(false);
        }
    });

    Effect::new(move |_| {
        if let Some(res) = del_block_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push("Block deleted.", ToastSeverity::Success);
                    blocks.refetch();
                }
                Err(e) => toast.push(format!("Error: {e}"), ToastSeverity::Error),
            }
            del_block_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <h1 class=style::page_title>"Blocks"</h1>

            // ── Add block group ─────────────────────────────────────────
            <section class=style::section>
                <h2 class=style::section_title>"Add Block Group"</h2>
                <div class=style::add_row>
                    <div class=style::add_field>
                        <label class=style::add_label>"Group Name"</label>
                        <input
                            class=style::add_input
                            placeholder="Group name…"
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
                        "+ Add Group"
                    </button>
                </div>
            </section>

            // ── Add block ───────────────────────────────────────────────
            <section class=style::section>
                <h2 class=style::section_title>"Add Block"</h2>
                <div class=style::add_row>
                    <div class=style::add_field>
                        <label class=style::add_label>"Block Group"</label>
                        <Suspense>
                            {move || groups.get().map(|grps| {
                                view! {
                                    <select
                                        class=style::add_select
                                        on:change=move |e| {
                                            if let Ok(id) = event_target_value(&e).parse::<i64>() {
                                                new_block_group_id.set(id);
                                            }
                                        }
                                    >
                                        <option value="0">"— select group —"</option>
                                        {grps.into_iter().map(|g| {
                                            let id = g.id;
                                            let name = g.name.clone();
                                            view! { <option value=id.to_string()>{name}</option> }
                                        }).collect_view()}
                                    </select>
                                }
                            })}
                        </Suspense>
                    </div>
                    <div class=style::add_field>
                        <label class=style::add_label>"Block Name"</label>
                        <input
                            class=style::add_input
                            placeholder="Block name…"
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
                        "+ Add Block"
                    </button>
                </div>
            </section>

            // ── Blocks table ────────────────────────────────────────────
            <div class=style::toolbar>
                <h2 class=style::section_title>"All Blocks"</h2>
                <input
                    class=style::search
                    type="search"
                    placeholder="Filter blocks…"
                    prop:value=move || search.get()
                    on:input=move |e| search.set(event_target_value(&e))
                />
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || {
                    blocks.get().map(|rows| {
                        if rows.is_empty() {
                            return view! {
                                <p class=style::empty>"No blocks yet."</p>
                            }
                            .into_any();
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
                                            <th>"Name"</th>
                                            <th>"Duration"</th>
                                            <th>"Stop Scheduling"</th>
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
                                                                "Delete Group"
                                                            </button>
                                                        </div>
                                                    </td>
                                                </tr>
                                                {block_rows.into_iter().map(|b| {
                                                    let id = b.id;
                                                    let name = b.name.clone();
                                                    let dur = format_duration_label(b.minutes);
                                                    let stop = crate::server::blocks::stop_scheduling_label(b.stop_scheduling);
                                                    let b_for_del = b.clone();
                                                    view! {
                                                        <tr>
                                                            <td>
                                                                <a href=format!("/scheduling/blocks/{id}")>{name}</a>
                                                            </td>
                                                            <td class=style::cell_muted>{dur}</td>
                                                            <td class=style::cell_muted>{stop}</td>
                                                            <td>
                                                                <div class=style::row_actions>
                                                                    <a class=style::btn_edit href=format!("/scheduling/blocks/{id}")>
                                                                        "Edit"
                                                                    </a>
                                                                    <button
                                                                        class=style::btn_delete
                                                                        on:click=move |_| {
                                                                            del_block_target.set(Some(b_for_del.clone()));
                                                                            del_block_open.set(true);
                                                                        }
                                                                    >
                                                                        "Delete"
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
                    })
                }}
            </Suspense>

            // ── Delete dialogs ──────────────────────────────────────────
            {move || del_group_target.get().map(|grp| {
                let id = grp.id;
                let name = grp.name.clone();
                view! {
                    <DeleteDialog
                        open=del_group_open
                        entity_name=name
                        on_confirm=Callback::new(move |confirmed: bool| {
                            if confirmed { del_group_action.dispatch(id); }
                            else { del_group_open.set(false); }
                        })
                    />
                }
            })}

            {move || del_block_target.get().map(|b| {
                let id = b.id;
                let name = b.name.clone();
                view! {
                    <DeleteDialog
                        open=del_block_open
                        entity_name=name
                        on_confirm=Callback::new(move |confirmed: bool| {
                            if confirmed { del_block_action.dispatch(id); }
                            else { del_block_open.set(false); }
                        })
                    />
                }
            })}
        </div>
    }
}
