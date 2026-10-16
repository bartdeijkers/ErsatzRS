//! Full identities, literal masks, immutable originals and indexed mappings.

use super::*;

const LATIN_A: [u16; 10] = [0, 0xf0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];

pub(super) fn previews(service: &EncodedTeletextAuthoredService, code: u8) -> Vec<TeletextRaster> {
    (124..127).map(|page| {
        let display = service.pages().iter().find_map(|candidate| match candidate {
            EncodedTeletextAuthoredPage::CompiledDisplay(candidate) if candidate.page().address() == address(page) => Some(candidate),
            _ => None,
        }).unwrap();
        polish_carrier::assert_base(display.normalized().original(),code);
        let raster = external_editor_probe::preview_page(service,page);
        let left = usize::from(display.normalized().original().display_geometry().left_width());
        for (y,row) in if code == 0x5b {MASK} else {LATIN_A}.into_iter().enumerate() {
            for x in 0..24 {
                let rgb = if row & (1 << (11-x/2)) == 0 {[0,0,0]} else {[15,15,15]};
                assert!(matches!(raster.pixel((left+7)*12+x,10+y),Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get() == rgb));
            }
        }
        raster
    }).collect()
}

pub(super) fn assert_originals(flow: &Flow) {
    for accepted in &flow.accepted {
        let service = accepted.service();
        assert_eq!(service.declared().semantic().base().pages.len(), 3);
        assert_eq!(service.declared().resource_inventory().len(), 4);
        assert!(service.compiler_drcs_resources().is_empty());
        for identity in authored_identities() {
            let table = service
                .declared()
                .semantic()
                .drcs()
                .subtables()
                .iter()
                .find(|table| table.identity() == identity)
                .unwrap();
            assert_eq!(table.glyphs().len(), 1);
            assert_eq!(table.glyphs()[0].slot.get(), 0);
            assert_eq!(table.glyphs()[0].bitmap.get(), [0; 10]);
        }
        for page in 124..127 {
            let metadata = &service.declared().semantic().page_enhancements()[&address(page)];
            assert_eq!(
                metadata.resources.drcs.page(TeletextDrcsTableScope::Normal),
                Some(normal_identity(page, 0).page())
            );
            assert_eq!(
                metadata.resources.drcs.page(TeletextDrcsTableScope::Global),
                Some(authored_identities()[3].page())
            );
            for role in [TeletextX27LinkRole::Drcs, TeletextX27LinkRole::Gdrcs] {
                assert_eq!(
                    metadata
                        .compositional_links
                        .unwrap()
                        .link(role)
                        .required_subpages()
                        .get(),
                    1
                );
            }
        }
        previews(service, b'A');
    }
}

pub(super) fn assert_service(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    let expected = flow.accepted[0]
        .service()
        .declared()
        .clone()
        .with_physical_base_overlays(
            expected_overlays(flow),
            input::encoding().validation.service,
        )
        .unwrap();
    assert_eq!(service.declared(), &expected);
    assert_eq!(
        service.magazine_units(),
        flow.accepted[0].service().magazine_units()
    );
    assert_eq!(service.compiler_drcs_resources().len(), 3);
    for page in 124..127 {
        let identity = normal_identity(page, 1);
        let resource = service
            .compiler_drcs_resources()
            .iter()
            .find(|resource| resource.table().identity() == identity)
            .unwrap();
        assert_eq!(resource.consumers(), [address(page)]);
        assert_eq!(resource.table().glyphs().len(), 1);
        assert_eq!(resource.table().glyphs()[0].slot.get(), 0);
        assert_eq!(resource.table().glyphs()[0].bitmap.get(), MASK);
        let links = resource.compositional_links(address(page)).unwrap();
        assert_eq!(
            links.link(TeletextX27LinkRole::Drcs).target(),
            TeletextX27LinkTarget::Page {
                page: identity.page()
            }
        );
        assert_eq!(
            links
                .link(TeletextX27LinkRole::Drcs)
                .required_subpages()
                .get(),
            3
        );
        assert_eq!(
            links
                .link(TeletextX27LinkRole::Gdrcs)
                .required_subpages()
                .get(),
            1
        );
        let display = service
            .pages()
            .iter()
            .find_map(|candidate| match candidate {
                EncodedTeletextAuthoredPage::CompiledDisplay(candidate)
                    if candidate.page().address() == address(page) =>
                {
                    Some(candidate)
                }
                _ => None,
            })
            .unwrap();
        assert!(display
            .normalized()
            .physical()
            .replay()
            .drcs_execution()
            .records()
            .iter()
            .any(|record| record.resource() == identity
                && record.reference().table == TeletextDrcsTableScope::Normal
                && record.reference().subtable.get() == 1
                && record.reference().slot.get() == 0
                && record.bitmap().get() == MASK));
        let projection =
            ersatzrs_ffmpeg::teletext::project_encoded_teletext_display(display.page(), 128)
                .unwrap();
        assert!(projection.records.iter().any(|record|matches!(record,TeletextEncodedDisplayRecord::BaseRow {row:1,cells} if cells[7] == 0x5b)));
    }
    for old in flow.accepted[0]
        .service()
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
    {
        let identity = old.resource_header().unwrap().identity();
        let current = service
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == identity)
            })
            .unwrap();
        assert_eq!(current.data_units(), old.data_units());
    }
}

pub(super) fn assert_mapping(exported: &ExportedOriginalTeletextDesign) {
    let assets = &exported.sidecar().project.assets;
    let mut mapped = Vec::new();
    for asset in assets {
        let TeletextExportAssetScope::DrcsTtiSet { resource_names } = &asset.scope else {
            continue;
        };
        let file = exported
            .files()
            .iter()
            .find(|file| file.name == asset.name)
            .unwrap();
        let tti = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
        let ordinals = tti
            .records()
            .iter()
            .filter_map(|record| match record {
                TeletextTtiRecord::SubpageOrdinal(ordinal) => Some(*ordinal),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ordinals,
            if asset.name == "drcs-set-global-1a1.tti" {
                vec![0]
            } else {
                vec![0, 1]
            }
        );
        for (ordinal, name) in resource_names.iter().enumerate() {
            let name = name.as_ref().unwrap();
            let raw = assets
                .iter()
                .find(|candidate| candidate.name == *name)
                .unwrap();
            let scope = serde_json::to_value(&raw.scope).unwrap();
            let identity = authored_identities()
                .into_iter()
                .chain((124..127).map(|page| normal_identity(page, 1)))
                .find(|identity| {
                    scope["identity"]["kind"] == json!(identity.kind())
                        && scope["identity"]["page"] == json!(identity.page())
                        && scope["identity"]["subpage"] == identity.logical_subpage().get()
                })
                .unwrap();
            assert_eq!(ordinal, usize::from(identity.logical_subpage().get()));
            assert_eq!(
                asset.name,
                format!(
                    "drcs-set-{}-{:x}{:02x}.tti",
                    if identity.kind() == TeletextResourceKind::Drcs {
                        "normal"
                    } else {
                        "global"
                    },
                    identity.page().magazine(),
                    identity.page().page()
                )
            );
            assert!(exported.files().iter().any(|file| file.name == *name));
            mapped.push(identity);
        }
    }
    mapped.sort();
    let mut expected = authored_identities();
    expected.extend((124..127).map(|page| normal_identity(page, 1)));
    expected.sort();
    assert_eq!(mapped, expected);
}
