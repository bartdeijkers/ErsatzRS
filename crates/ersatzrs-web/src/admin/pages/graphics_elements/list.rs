use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::graphics_elements::{
    list_graphics_element_rows, GraphicsElementListRow,
};
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};

stylance::import_crate_style!(style, "src/admin/pages/graphics_elements/list.module.css");

#[component]
pub fn GraphicsElementsListPage() -> impl IntoView {
    let rows = Resource::new_blocking(|| (), |()| list_graphics_element_rows());

    view! {
        <div class=style::page data-graphics-elements-page="list">
            <div class=style::action_bar>
                <a href="/graphics-elements/add" class=style::btn_add data-add-graphics-element>
                    {tr(TextKey::AddGraphicsElement)}
                </a>
            </div>
            <section class=style::content>
                <h1 class=style::page_title>{tr(TextKey::GraphicsElements)}</h1>
                <p class=style::help>{tr(TextKey::GraphicsExistingDrawvgHelp)}</p>
                <p class=style::help>{tr(TextKey::GraphicsStreamRestartHelp)}</p>
                <div class=style::divider/>
                <Suspense fallback=admin_loading_resource_view>
                    {move || {
                        rows.get().map(|result| admin_result_view(result, graphics_elements_table))
                    }}
                </Suspense>
            </section>
        </div>
    }
}

fn graphics_elements_table(rows: Vec<GraphicsElementListRow>) -> AnyView {
    if rows.is_empty() {
        return view! {
            <p class=style::empty data-graphics-elements-empty>{tr(TextKey::NoGraphicsElements)}</p>
        }
        .into_any();
    }

    view! {
        <div class=style::table_wrap>
            <table class=style::table data-graphics-elements-table>
                <thead>
                    <tr>
                        <th>{tr(TextKey::Name)}</th>
                        <th>{tr(TextKey::GraphicsElementType)}</th>
                        <th>{tr(TextKey::Renderer)}</th>
                        <th><span class=style::visually_hidden>{tr(TextKey::Actions)}</span></th>
                    </tr>
                </thead>
                <tbody>
                    {rows.into_iter().map(graphics_element_row).collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

fn graphics_element_row(row: GraphicsElementListRow) -> impl IntoView {
    let name = if row.name.trim().is_empty() {
        tr(TextKey::UntitledItem).to_owned()
    } else {
        row.name
    };
    let kind = graphics_element_kind_label(row.kind);
    let renderer = row
        .renderer
        .map_or_else(|| tr(TextKey::Unavailable), renderer_label);

    view! {
        <tr
            data-graphics-element-row=row.id.to_string()
            data-graphics-element-kind=row.kind.to_string()
        >
            <td>{name}</td>
            <td class=style::muted>{kind}</td>
            <td class=style::muted data-graphics-element-renderer>{renderer}</td>
            <td class=style::actions>
                <a
                    href=format!("/graphics-elements/{}", row.id)
                    class=style::edit_link
                    aria-label=tr(TextKey::EditGraphicsElement)
                    data-graphics-element-edit="true"
                >
                    {tr(TextKey::Edit)}
                </a>
            </td>
        </tr>
    }
}

fn graphics_element_kind_label(kind: i32) -> &'static str {
    match kind {
        0 => tr(TextKey::GraphicsElementKindImage),
        1 => tr(TextKey::GraphicsElementKindText),
        2 => tr(TextKey::GraphicsElementKindSubtitle),
        3 => tr(TextKey::GraphicsElementKindMotion),
        4 => tr(TextKey::GraphicsElementKindScript),
        _ => tr(TextKey::Unavailable),
    }
}

fn renderer_label(renderer: ersatzrs_domain::TextGraphicsRenderer) -> &'static str {
    match renderer {
        ersatzrs_domain::TextGraphicsRenderer::Drawvg => tr(TextKey::DrawvgRendererOption),
        ersatzrs_domain::TextGraphicsRenderer::Drawtext => tr(TextKey::DrawtextRendererOption),
    }
}
