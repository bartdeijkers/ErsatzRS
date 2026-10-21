//! Retained live sibling banks plus one independently authored idle display.
use super::*;

pub(super) fn display() -> ersatzrs_domain::TeletextPageAddress {
    channel_documents::address(227, 32)
}
fn object_resource(role: TeletextX27LinkRole) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        role.kind(),
        TeletextRawDataPageNumber::new(
            1,
            if role == TeletextX27LinkRole::Pop {
                0xe0
            } else {
                0xe1
            },
        )
        .unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap()
}
pub(super) fn target(role: TeletextX27LinkRole) -> ersatzrs_domain::TeletextRawDataPageNumber {
    match role {
        TeletextX27LinkRole::Pop | TeletextX27LinkRole::Gpop => object_resource(role).page(),
        TeletextX27LinkRole::Drcs => bank(TeletextDrcsTableScope::Normal, 0).page(),
        TeletextX27LinkRole::Gdrcs => bank(TeletextDrcsTableScope::Global, 0).page(),
    }
}
pub(super) fn declaration(role: TeletextX27LinkRole) -> TeletextX27LinkDeclaration {
    TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: target(role) },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(
            if matches!(role, TeletextX27LinkRole::Drcs | TeletextX27LinkRole::Gdrcs) {
                3
            } else {
                1
            },
        ),
    )
    .unwrap()
}
fn response(theme: &str, page: u16, subcode: u16, explicit: bool, selected: bool) -> Value {
    let mut value = super::super::source::original(theme, page, subcode);
    let mut page = value["service"]["pages"][0].clone();
    page["address"] = json!(display());
    value["service"]["pages"].as_array_mut().unwrap().push(page);
    value["service"]["mot_selections"]
        .as_array_mut()
        .unwrap()
        .push(json!({"display":display(),"mot":null}));
    let unused = json!({"target":{"kind":"unspecified","magazine":2},"validity":"level25_only","required_subpages":65535});
    let links = if selected {
        json!({"pop":declaration(TeletextX27LinkRole::Pop),"gpop":declaration(TeletextX27LinkRole::Gpop),
            "drcs":declaration(TeletextX27LinkRole::Drcs),"gdrcs":declaration(TeletextX27LinkRole::Gdrcs)})
    } else if explicit {
        json!({"pop":unused,"gpop":unused,"drcs":unused,"gdrcs":unused})
    } else {
        Value::Null
    };
    value["service"]["page_enhancements"].as_array_mut().unwrap().push(json!({
        "address":display(),"presentation":null,"compositional_links":links,"local_program":null,
        "resources":{"objects":{"public":if selected {json!(target(TeletextX27LinkRole::Pop))} else {Value::Null},
            "global":if selected {json!(target(TeletextX27LinkRole::Gpop))} else {Value::Null}},
            "drcs":{"normal":if selected {json!(target(TeletextX27LinkRole::Drcs))} else {Value::Null},
                "global":if selected {json!(target(TeletextX27LinkRole::Gdrcs))} else {Value::Null}},
            "defaults":[null,null,null,null]}}));
    value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "page":display(),"regions":[{"id":"idle-retained-target","kind":"zone","area":"main",
            "row":1,"column":0,"width":40,"height":24}],"dynamic_fields":[]}));
    for role in [TeletextX27LinkRole::Pop, TeletextX27LinkRole::Gpop] {
        let resource = object_resource(role);
        value["service"]["empty_object_pages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind":resource.kind(),"page":resource.page(),"subpage":0}));
    }
    serde_json::from_value::<ersatzrs_addon_contract::TeletextPagesV3Response>(value.clone())
        .unwrap();
    value
}
pub(super) fn absent(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, false, false)
}
pub(super) fn explicit(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, true, false)
}
pub(super) fn selected_absent(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, false, true)
}
pub(super) fn selected_explicit(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, true, true)
}
pub(super) fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = super::super::source::encoding();
    for role in [TeletextX27LinkRole::Pop, TeletextX27LinkRole::Gpop] {
        policy
            .transmissions
            .push(TeletextServiceResourceTransmission {
                resource: object_resource(role),
                counter: TeletextResourceUpdateCounter::new(1).unwrap(),
                update: TeletextResourceFullUpdate::Changed,
                last_subpage: true,
            });
    }
    policy
}

fn metadata(value: &mut Value) -> &mut Value {
    value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|metadata| metadata["address"] == json!(display()))
        .unwrap()
}
pub(super) fn alternate_declaration() -> TeletextX27LinkDeclaration {
    TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page {
            page: super::super::source::alternate(TeletextDrcsTableScope::Normal, 0).page(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(3),
    )
    .unwrap()
}
pub(super) fn retargeted(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = selected_absent(theme, page, subcode);
    let metadata = metadata(&mut value);
    metadata["compositional_links"]["drcs"] = json!(alternate_declaration());
    metadata["resources"]["drcs"]["normal"] =
        json!(super::super::source::alternate(TeletextDrcsTableScope::Normal, 0).page());
    value
}
pub(super) fn partial(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = selected_absent(theme, page, subcode);
    let metadata = metadata(&mut value);
    metadata["compositional_links"]["drcs"] = json!({"target":{"kind":"unspecified","magazine":2},
        "validity":"level25_only","required_subpages":65535});
    metadata["resources"]["drcs"]["normal"] = Value::Null;
    value
}
pub(super) fn normal(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = explicit(theme, page, subcode);
    let metadata = metadata(&mut value);
    metadata["compositional_links"]["drcs"] = json!(declaration(TeletextX27LinkRole::Drcs));
    metadata["resources"]["drcs"]["normal"] = json!(target(TeletextX27LinkRole::Drcs));
    value
}
