//! Host-only output comparison grants no fresh publication authority.

use super::*;
use chrono::{Duration, TimeZone as _};

fn selection(host: bool) -> TeletextEffectiveChannelSelectionSnapshot {
    let start = Utc.with_ymd_and_hms(2026, 11, 15, 0, 0, 0).unwrap();
    let mut addon = super::super::teletext_custom_pages_selection(9);
    addon.addon_id = "org.example.base".to_owned();
    addon.installation.addon_id.clone_from(&addon.addon_id);
    let mut bindings = vec![
        TeletextEffectiveBindingSnapshot::Base(addon),
        TeletextEffectiveBindingSnapshot::Scripted(TeletextPublishedOverride {
            playout_id: 12,
            interval: TeletextOverrideInterval {
                start,
                finish: start + Duration::minutes(5),
                state: TeletextOverrideState::Off {
                    channel_id: 9,
                    addon_id: "org.example.lua".to_owned(),
                },
            },
        }),
    ];
    if host {
        bindings.insert(
            0,
            TeletextEffectiveBindingSnapshot::Base(super::super::teletext_custom_pages_selection(
                9,
            )),
        );
    }
    TeletextEffectiveChannelSelectionSnapshot {
        channel_id: 9,
        bindings,
        valid_until: Some(start + Duration::minutes(5)),
    }
}

#[test]
fn published_effective_host_presence_or_absence_is_frozen_in_both_directions() {
    for before in [false, true] {
        for after in [false, true] {
            let published = selection(before);
            let current = selection(after);
            assert!(teletext_published_effective_selection_matches(
                &current, &published
            ));
            assert!(teletext_published_effective_selection_matches(
                &published, &current
            ));
            assert_eq!(published == current, before == after);
        }
    }
}

#[test]
fn published_effective_host_exception_rejects_scope_origin_dependency_and_order_drift() {
    let published = selection(true);
    for variant in 0..12 {
        let mut current = published.clone();
        match variant {
            0 => current.channel_id += 1,
            1 => current.valid_until = current.valid_until.map(|at| at + Duration::milliseconds(1)),
            2 => current.valid_until = None,
            3 => current.bindings.swap(0, 1),
            4 => current.bindings.insert(0, current.bindings[0].clone()),
            5 => {
                let TeletextEffectiveBindingSnapshot::Base(host) = &mut current.bindings[0] else {
                    unreachable!()
                };
                host.profile_revision += 1;
            }
            6 => {
                let TeletextEffectiveBindingSnapshot::Base(base) = &mut current.bindings[1] else {
                    unreachable!()
                };
                base.installation.state_revision.push('x');
            }
            7 => {
                let TeletextEffectiveBindingSnapshot::Base(base) = &mut current.bindings[1] else {
                    unreachable!()
                };
                base.channel_id += 1;
            }
            8 => {
                let TeletextEffectiveBindingSnapshot::Scripted(scope) = &mut current.bindings[2]
                else {
                    unreachable!()
                };
                scope.playout_id += 1;
            }
            9 => {
                let TeletextEffectiveBindingSnapshot::Scripted(scope) = &mut current.bindings[2]
                else {
                    unreachable!()
                };
                scope.interval.start += Duration::milliseconds(1);
            }
            10 => {
                let TeletextEffectiveBindingSnapshot::Scripted(scope) = &mut current.bindings[2]
                else {
                    unreachable!()
                };
                scope.interval.finish += Duration::milliseconds(1);
            }
            _ => {
                let TeletextEffectiveBindingSnapshot::Scripted(mut scope) =
                    current.bindings.pop().unwrap()
                else {
                    unreachable!()
                };
                scope.interval.state = TeletextOverrideState::Off {
                    channel_id: 9,
                    addon_id: super::super::TELETEXT_CUSTOM_PAGES_PRODUCER_ID.to_owned(),
                };
                current.bindings[0] = TeletextEffectiveBindingSnapshot::Scripted(scope);
            }
        }
        assert!(
            !teletext_published_effective_selection_matches(&current, &published),
            "variant {variant}"
        );
        assert!(
            !teletext_published_effective_selection_matches(&published, &current),
            "reverse variant {variant}"
        );
    }
}

#[test]
fn original_host_still_consumes_a_published_producer_slot() {
    let mut bindings = Vec::new();
    for index in 0..crate::MAX_TELETEXT_PAGE_RECORDS {
        let mut base = crate::teletext_custom_pages_selection(1);
        base.addon_id = format!("org.example.capacity.p{index:04}");
        base.installation.addon_id.clone_from(&base.addon_id);
        bindings.push(TeletextEffectiveBindingSnapshot::Base(base));
    }
    let original = TeletextEffectiveChannelSelectionSnapshot {
        channel_id: 1,
        bindings,
        valid_until: None,
    };
    assert!(teletext_published_effective_selection_matches(
        &original, &original
    ));
    let mut with_host = original.clone();
    with_host.bindings.insert(
        0,
        TeletextEffectiveBindingSnapshot::Base(crate::teletext_custom_pages_selection(1)),
    );
    assert!(!teletext_published_effective_selection_matches(
        &original, &with_host
    ));
    assert!(!teletext_published_effective_selection_matches(
        &with_host, &original
    ));
    with_host.bindings.pop();
    assert!(teletext_published_effective_selection_matches(
        &with_host, &with_host
    ));
}
