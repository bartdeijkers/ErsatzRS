//! Genuine finite Scripted absence, without a producer or editorial receipt.

use super::super::super::effective_authoring;
use super::*;
use ersatzrs_domain::TeletextEffectiveBindingSnapshot;

pub(super) async fn prepare(base: &Base) -> Scoped {
    let fixture = &base.fixture.fixture;
    let calls = base.fixture.calls();
    let script = format!("assert(teletext_enabled('{ADDON}',false).ok)\nadd_duration('clip','00:00:35',{{trim=true,stop_before_end=true}})");
    let off = schedule(fixture, 1, &script, None).await;
    assert_eq!(off.added_items.len(), 1);
    assert_eq!(off.teletext_intervals.len(), 1);
    let interval = &off.teletext_intervals[0];
    assert!(
        matches!(&interval.state, TeletextOverrideState::Off {channel_id:1,addon_id} if addon_id==ADDON)
    );
    assert_eq!(interval.start, off.added_items[0].start);
    assert_eq!(interval.finish, off.added_items[0].finish);
    assert_eq!(
        interval.finish - interval.start,
        chrono::Duration::seconds(35)
    );
    let tail = schedule(fixture, 1, "add_count('clip',4)", Some(&off)).await;
    assert!(tail.teletext_intervals.is_empty());
    assert_eq!(tail.added_items[0].start, interval.finish);
    let (identity, raster, _) = effective_authoring::prepare(&base.fixture, 1).await;
    assert!(raster.is_none());
    let preview = effective_authoring::preview(&base.fixture, identity, true).await;
    assert!(preview.is_absent() && preview.raster().is_none());
    assert!(preview.pages().is_empty() && preview.navigation().is_empty());
    assert!(preview.authoring().is_empty() && preview.object_resources().is_empty());
    drop(preview);
    cold::publish_with_owner(&base.fixture, identity, None).await;
    let accepted = cold::recover(fixture, 1).await;
    assert!(accepted.service().is_none());
    let original = original(fixture, identity).await;
    let saved =
        decode_teletext_effective_design_documents(&original.document, designs::budget()).unwrap();
    assert!(saved.editorial_overlay().is_none());
    assert!(saved.producer_bindings().is_empty() && saved.documents().is_empty());
    assert!(saved.sources().is_empty() && saved.entry_addon_id().is_none());
    assert!(saved.entry().is_none() && saved.resource_transmissions().is_empty());
    assert_eq!(saved.selection().valid_until, Some(interval.finish));
    assert_eq!(accepted.selection(), saved.selection());
    assert_eq!(
        base.fixture.calls(),
        calls,
        "absence never invokes a producer"
    );
    assert!(Utc::now() < interval.finish);
    Scoped {
        accepted,
        original,
        finish: interval.finish,
    }
}

pub(super) fn record(scoped: &Scoped) -> Value {
    let original =
        decode_teletext_effective_design_documents(&scoped.original.document, designs::budget())
            .unwrap();
    assert!(original.editorial_overlay().is_none());
    assert_eq!(scoped.original.revision, 1);
    assert!(scoped.accepted.service().is_none());
    assert_eq!(scoped.accepted.selection().bindings.len(), 1);
    let TeletextEffectiveBindingSnapshot::Scripted(published) =
        &scoped.accepted.selection().bindings[0]
    else {
        panic!("genuine Off origin required")
    };
    let TeletextOverrideState::Off {
        channel_id,
        addon_id,
    } = &published.interval.state
    else {
        panic!("genuine Off state required")
    };
    assert_eq!(*channel_id, 1);
    assert_eq!(published.interval.finish, scoped.finish);
    json!({"design_id":scoped.original.design_id,"revision":scoped.original.revision,"digest":scoped.original.digest,
        "generation_id":scoped.accepted.generation().id,"request_id":scoped.accepted.generation().request.id,
        "playout_id":published.playout_id,"addon_id":addon_id,"start":published.interval.start.to_rfc3339(),"finish":scoped.finish.to_rfc3339(),
        "state":"off","producer_count":0,"owner":null,"has_editorial_overlay":false})
}
