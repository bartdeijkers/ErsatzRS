//! Read-only editing boundaries from the same original as the selected raster.

use ersatzrs_addon_contract::{
    TeletextAuthoringRegionAreaWire as Area, TeletextAuthoringRegionKindWire as Kind,
};
use leptos::prelude::*;

use super::style;
use crate::admin::{
    i18n::{tr, TextKey},
    server_fns::teletext_drafts::{TeletextDraftAuthoringData, TeletextDraftPreviewData},
};

pub(super) fn boundaries(data: &TeletextDraftPreviewData) -> AnyView {
    if data.owner.is_none() {
        return ().into_any();
    }
    let producers = data.authoring.iter().map(producer).collect_view();
    view! {
        <section class=style::editorial data-testid="teletext-preview-authoring">
            <h3>{tr(TextKey::TeletextPreviewEditorialTitle)}</h3>
            <p>{tr(TextKey::TeletextPreviewEditorialHelp)}</p>
            <p hidden=!data.authoring.is_empty()>{tr(TextKey::TeletextPreviewNoEditorialMetadata)}</p>
            {producers}
        </section>
    }.into_any()
}

fn producer(data: &TeletextDraftAuthoringData) -> AnyView {
    let regions = data
        .regions
        .iter()
        .map(|region| {
            let permission = match region.kind {
                Kind::Free => TextKey::TeletextPreviewFree,
                Kind::Zone => TextKey::TeletextPreviewZone,
                Kind::Generated => TextKey::TeletextPreviewGenerated,
            };
            let area = match region.area {
                Area::Main => TextKey::TeletextPreviewPanelMain,
                Area::LeftPanel => TextKey::TeletextPreviewPanelLeft,
                Area::RightPanel => TextKey::TeletextPreviewPanelRight,
            };
            // Source validation bounds each rectangle. Widen before inclusive-end
            // arithmetic so the projection itself never wraps a byte coordinate.
            let last_row = u16::from(region.row) + u16::from(region.height) - 1;
            let last_column = u16::from(region.column) + u16::from(region.width) - 1;
            let extent = format!(
                "{}–{} / {}–{}",
                region.row, last_row, region.column, last_column
            );
            view! {
                <tr data-testid="teletext-preview-region" data-region=region.id.clone()>
                    <th scope="row">{region.id.clone()}</th>
                    <td>{tr(area)}</td><td>{extent}</td><td>{tr(permission)}</td>
                </tr>
            }
        })
        .collect_view();
    let fields = data
        .dynamic_fields
        .iter()
        .map(|field| {
            view! {
                <tr data-testid="teletext-preview-dynamic-field">
                    <th scope="row">{field.id.clone()}</th>
                    <td>{field.region.clone()}</td><td>{field.binding.clone()}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <section data-testid="teletext-preview-authoring-producer" data-addon-id=data.addon_id.clone()>
            <h4>{data.addon_id.clone()}</h4>
            <dl class=style::location>
                <dt>{tr(TextKey::Template)}</dt>
                <dd data-testid="teletext-preview-template">{data.template.id.clone()}" · "{tr(TextKey::Revision)}" "{data.template.revision.clone()}</dd>
                <dt>{tr(TextKey::Theme)}</dt>
                <dd data-testid="teletext-preview-theme">{data.theme.id.clone()}</dd>
            </dl>
            <table class=style::editorial_table>
                <caption>{tr(TextKey::TeletextPreviewRegion)}</caption>
                <thead><tr>
                    <th scope="col">{tr(TextKey::TeletextPreviewRegion)}</th>
                    <th scope="col">{tr(TextKey::TeletextPreviewArea)}</th>
                    <th scope="col">{tr(TextKey::TeletextPreviewExtent)}</th>
                    <th scope="col">{tr(TextKey::TeletextPreviewPermission)}</th>
                </tr></thead>
                <tbody>{regions}</tbody>
            </table>
            <table class=style::editorial_table hidden=data.dynamic_fields.is_empty()>
                <caption>{tr(TextKey::TeletextPreviewDynamicField)}</caption>
                <thead><tr>
                    <th scope="col">{tr(TextKey::TeletextPreviewDynamicField)}</th>
                    <th scope="col">{tr(TextKey::TeletextPreviewRegion)}</th>
                    <th scope="col">{tr(TextKey::TeletextPreviewBinding)}</th>
                </tr></thead>
                <tbody>{fields}</tbody>
            </table>
        </section>
    }.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_addon_contract::{
        TeletextAuthoringDynamicFieldWire, TeletextAuthoringRegionWire,
        TeletextAuthoringTemplateWire, TeletextAuthoringThemeWire,
    };

    #[test]
    fn editorial_boundaries_identify_permissions_panels_and_bindings_without_controls() {
        let data = TeletextDraftAuthoringData {
            producer_index: 1,
            binding_index: 2,
            addon_id: "org.example.synthetic".to_owned(),
            template: TeletextAuthoringTemplateWire {
                id: "<template>".to_owned(),
                revision: "r1".to_owned(),
            },
            theme: TeletextAuthoringThemeWire {
                id: "night".to_owned(),
                overrides: Vec::new(),
            },
            regions: [
                (Kind::Free, Area::Main),
                (Kind::Zone, Area::LeftPanel),
                (Kind::Generated, Area::RightPanel),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (kind, area))| TeletextAuthoringRegionWire {
                id: format!("region-{index}"),
                kind,
                area,
                row: 2,
                column: 0,
                width: 3,
                height: 2,
            })
            .collect(),
            dynamic_fields: vec![TeletextAuthoringDynamicFieldWire {
                id: "guide-title".to_owned(),
                region: "region-2".to_owned(),
                binding: "guide.title".to_owned(),
            }],
        };
        let context = Owner::new();
        let html = context.with(|| producer(&data).to_html());
        for value in [
            "Free content",
            "Fillable zone",
            "Automatically generated",
            "Main page",
            "Left panel",
            "Right panel",
            "2–3 / 0–2",
            "guide-title",
            "region-2",
            "guide.title",
            "night",
            "&lt;template&gt;",
        ] {
            assert!(html.contains(value), "missing {value}: {html}");
        }
        assert!(!html.contains("<template>"));
        for control in ["<form", "<input", "<button", "<select", "<textarea"] {
            assert!(
                !html.contains(control),
                "projection grants no mutation: {control}"
            );
        }
        assert_eq!(
            html.matches("data-testid=\"teletext-preview-region\"")
                .count(),
            3
        );
    }
}
