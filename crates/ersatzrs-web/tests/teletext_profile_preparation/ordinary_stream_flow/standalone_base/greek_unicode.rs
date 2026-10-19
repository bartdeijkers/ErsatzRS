//! Existing application/profile/draft/publication path for native Greek input.
//! No new editor, browser, stream or platform acceptance is asserted here.
use super::*;

fn greek_response(theme: &str) -> Value {
    let mut value = response(theme);
    let mut presentation =
        super::super::themed_pages(theme)["magazines"][0]["presentation"].clone();
    presentation["primary"] = json!(0x37);
    for page in value["pages"].as_array_mut().unwrap() {
        page["national_option"] = json!(7);
        for (index, row) in page["rows"].as_array_mut().unwrap().iter_mut().enumerate() {
            row["runs"][0]["content"]["text"] = json!(["Αθήνα 200", "Ζ", "Ωμέγα"][index]);
        }
    }
    for page in value["page_enhancements"].as_array_mut().unwrap() {
        page["presentation"] = presentation.clone();
        page["local_program"] = Value::Null;
    }
    value
}

#[tokio::test]
#[ignore = "requires pinned native FFmpeg9 tools for the existing source fixture; native app proof only"]
async fn greek_profiles_validate_preview_publish_and_reopen_without_banks() {
    let tools = Tools::load().await;
    let (mut fixture, _) = installed_profiles_with(&tools, greek_response, 12).await;
    let before = designs::config(&fixture).await;
    let package = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    let pending = request_base(&fixture, 1, None).await;
    for channel in [1, 2] {
        assert!(fixture
            .state
            .application
            .accepted_teletext_generation(channel)
            .await
            .unwrap()
            .is_none());
    }
    let accepted = [
        publish_base(&fixture, pending, None).await,
        accept_base(&fixture, 2).await,
    ];
    for candidate in &accepted {
        assert_base(candidate.service());
        for page in &candidate.service().declared().semantic().base().pages {
            assert_eq!(page.national_option.get(), 7);
            assert!(
                matches!(&page.rows[0].runs[0].content,ersatzrs_domain::TeletextRunContent::Text {text} if text=="Αθήνα 200")
            );
        }
    }
    assert_ne!(
        designs::units(accepted[0].service()),
        designs::units(accepted[1].service())
    );
    fixture = fixture.reopen().await;
    for (index, channel) in [1, 2].into_iter().enumerate() {
        let recovered = reload(&fixture, channel).await;
        assert_eq!(recovered.generation(), accepted[index].generation());
        assert_eq!(
            designs::units(recovered.service()),
            designs::units(accepted[index].service())
        );
        assert_base(recovered.service());
    }
    assert_eq!(designs::config(&fixture).await, before);
    assert_eq!(
        calculate_addon_content_sha256(&fixture.root.path().join("package"))
            .await
            .unwrap(),
        package
    );
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        b"day\nnight\n"
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
