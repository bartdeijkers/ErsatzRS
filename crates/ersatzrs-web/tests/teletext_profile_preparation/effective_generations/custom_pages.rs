//! Actual Lua/native frozen originals; ordinary HTTP/AV remains a separate proof.

use super::super::custom_pages as host_pages;
use super::*;
use ersatzrs_web::host::teletext_effective_publications::NativeEffectiveTeletextPublicationError;

async fn stored_original(fixture: &Fixture, id: i64) -> ersatzrs_domain::TeletextDesignRevision {
    fixture
        .state
        .application
        .teletext_design_revision(id, 1, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn actual_lua_custom_originals_preserve_published_presence_and_absence_after_reopen() {
    for lua_on in [true, false] {
        for published_host in [false, true] {
            let (mut fixture, _, day, _) = originals::installed(true).await;
            effective::publish(&fixture, 1, lua_on.then_some(&day)).await;
            let element = if published_host {
                Some(host_pages::link(&fixture, "FIXED", None).await)
            } else {
                None
            };
            let entry = if lua_on {
                Some(ADDON)
            } else if published_host {
                Some(ersatzrs_domain::TELETEXT_CUSTOM_PAGES_PRODUCER_ID)
            } else {
                None
            };
            let first = originals::saved_design(
                originals::save(
                    &fixture,
                    effective::candidate(&fixture, 1, entry).await.unwrap(),
                    None,
                    designs::budget(),
                )
                .await
                .unwrap(),
            );
            let request = pending::request(&fixture, &first, None).await;
            let before = accepted(accept(&fixture, &request, 1, None).await.unwrap());
            let stored = stored_original(&fixture, first.head().id).await;
            let calls = std::fs::read(fixture.root.path().join("authoring-calls")).ok();
            let before_units = before.service().map(|service| designs::units(service));
            assert_eq!(before.service().is_some(), lua_on || published_host);
            if published_host {
                assert!(host_pages::packet_rows(before.service().unwrap())
                    .iter()
                    .any(|row| row.windows(5).any(|word| word == b"FIXED")));
            }
            if let Some(element) = element {
                // Actual removal cascades the last current link; the old source
                // and its element revision remain in the immutable original.
                fixture
                    .state
                    .application
                    .delete_teletext_element(element)
                    .await
                    .unwrap();
            } else {
                host_pages::link(&fixture, "LATER", None).await;
            }
            let republish = request_effective_original_teletext_design_publication(
                &fixture.state.application,
                pending::target(&first, Some(request.id)),
                designs::budget(),
                channel_documents::policy(),
                designs::deadline(),
            )
            .await;
            assert!(matches!(
                republish,
                Err(NativeEffectiveTeletextPublicationError::Design(_))
            ));
            fixture = fixture.reopen().await;
            let after = recover(&fixture, 1).await.unwrap().unwrap();
            assert_eq!(after.generation(), before.generation());
            assert_eq!(after.selection(), before.selection());
            assert_eq!(
                after.service().map(|service| designs::units(service)),
                before_units
            );
            assert_eq!(
                std::fs::read(fixture.root.path().join("authoring-calls")).ok(),
                calls
            );
            let unchanged = stored_original(&fixture, first.head().id).await;
            assert_eq!(unchanged.digest, stored.digest);
            assert_eq!(unchanged.document, stored.document);
            assert!(fixture
                .state
                .application
                .accepted_teletext_generation(1)
                .await
                .unwrap()
                .is_none());
            assert!(fixture.playout.try_recv().is_err());
        }
    }
}
