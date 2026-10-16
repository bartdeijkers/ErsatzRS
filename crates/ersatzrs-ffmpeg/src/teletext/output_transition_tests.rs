//! Actual window/PSI/PES transitions preserve the continuing physical source.

use super::*;
use crate::teletext::{
    authored_service_tests as authored,
    service_update::tests::{candidate, spec},
    transport_tests::draft,
    TeletextResourceFullUpdate,
};
use ersatzrs_domain::*;
use mpeg2ts_reader::mpegts_crc;

pub(in crate::teletext) fn simple(
    language: &str,
    magazine: Option<u8>,
) -> Arc<EncodedTeletextAuthoredService> {
    simple_with_text(language, magazine, "ALPHA")
}

pub(in crate::teletext) fn simple_with_text(
    language: &str,
    magazine: Option<u8>,
    text: &str,
) -> Arc<EncodedTeletextAuthoredService> {
    let mut page = draft(200, 0, false);
    page.language = TeletextLanguage::new(language.to_owned()).unwrap();
    page.rows[0].runs[0].content = TeletextRunContent::Text { text: text.into() };
    let mut presentation =
        crate::teletext::presentation::western_latin_v1_presentation(page.national_option).unwrap();
    if let Some(value) = magazine {
        presentation.palette.clut2[0] = TeletextRgb::new([value, 2, 3]).unwrap();
    }
    let owner = page.address;
    let input = TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            initial_page: owner.page,
            language: page.language.clone(),
            pages: vec![page],
        },
        pages: vec![TeletextResolvedPageEnhancementDraft {
            address: owner,
            presentation: None,
            compositional_links: None,
            local_program: None,
            resources: TeletextResolvedPageResourceSelection {
                objects: TeletextResolvedObjectPageLinks {
                    public: None,
                    global: None,
                },
                drcs: TeletextDrcsPageLinks::unlinked(),
                defaults: [None; 4],
            },
        }],
        magazines: magazine
            .map(|_| TeletextMagazinePresentationDraft {
                magazine: 2,
                presentation,
            })
            .into_iter()
            .collect(),
        objects: vec![],
        drcs: vec![],
    };
    let semantic = TeletextSemanticService::new(input, authored::semantic_limits()).unwrap();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner,
            mot: None,
        }],
        vec![],
        authored::declaration_limits(),
    )
    .unwrap();
    Arc::new(
        super::super::encode_teletext_authored_service(declared, &[], &authored::limits()).unwrap(),
    )
}

fn gate(
    service: Arc<EncodedTeletextAuthoredService>,
    frames: u64,
) -> (TeletextOutputGate, Vec<[u8; 188]>) {
    let mut policy = tests::config();
    policy.max_buffer_packets = 32_768;
    policy.clock.max_window_packets = 32_768;
    policy.max_hold = Duration::from_secs(10);
    let source = tests::source(frames);
    let mut gate = TeletextOutputGate::new_authored_shared(policy, service).unwrap();
    gate.push(&tests::bytes(&source), Duration::ZERO).unwrap();
    (gate, source)
}

fn deliver(
    gate: &mut TeletextOutputGate,
    intervals: usize,
) -> (TeletextOutputAcknowledgement, Vec<[u8; 188]>, Duration) {
    let end = gate.window_end(intervals).unwrap().unwrap();
    let count = usize::try_from(end - gate.source_start).unwrap();
    let now = gate.due_at(gate.source_start).unwrap();
    assert!(gate.prepare_window(count, now).unwrap());
    let schedule = gate.pending_schedule().unwrap();
    let at = schedule.last_due();
    let bytes = gate.pending_packets(schedule.first_due()).unwrap().to_vec();
    let receipt = gate.acknowledge_window_with_receipt(schedule, at).unwrap();
    (receipt, bytes, at)
}

fn cycle(
    gate: &mut TeletextOutputGate,
) -> (TeletextOutputAcknowledgement, Vec<[u8; 188]>, Duration) {
    let mut bytes = Vec::new();
    for _ in 0..100 {
        let (receipt, window, now) = deliver(gate, 1);
        bytes.extend(window);
        if receipt.is_cycle_boundary() && receipt.contains_complete_cycle() {
            return (receipt, bytes, now);
        }
    }
    panic!("bounded fixture must finish its actual cycle");
}

fn pid(packet: &[u8; 188]) -> u16 {
    u16::from(Packet::new(packet).pid())
}

fn assert_source_positions(output: &[[u8; 188]], input: &[[u8; 188]], first: usize) {
    for (offset, packet) in output.iter().enumerate() {
        if !matches!(pid(&input[first + offset]), 3337 | 8191) {
            assert_eq!(packet, &input[first + offset]);
        }
    }
}

fn section(packet: &[u8; 188]) -> &[u8] {
    assert_eq!(pid(packet), 3337);
    let payload = Packet::new(packet).payload().unwrap();
    assert_eq!(payload[0], 0);
    let bytes = &payload[1..];
    let length = 3 + (usize::from(bytes[1] & 15) << 8 | usize::from(bytes[2]));
    &bytes[..length]
}

#[test]
fn terminator_followed_by_next_cycle_is_not_a_replacement_boundary() {
    let old = simple("nld", None);
    let (mut gate, _) = gate(Arc::clone(&old), 12);
    let (receipt, _, now) = deliver(&mut gate, 4);
    assert!(receipt.contains_complete_cycle());
    assert!(!receipt.is_cycle_boundary());
    assert!(gate.completed_cycle);
    let before = (
        gate.source_start,
        gate.batch_ticks,
        gate.installer.as_ref().unwrap().next_continuity_counter(),
    );
    assert_eq!(
        gate.replace_authored_at_acknowledged_boundary(&receipt, &simple("nld", None), now),
        Err(TeletextOutputError::Boundary)
    );
    assert!(gate.failure.is_none());
    assert!(Arc::ptr_eq(
        gate.carousel
            .as_ref()
            .unwrap()
            .authored_service_shared()
            .unwrap(),
        &old
    ));
    assert_eq!(
        (
            gate.source_start,
            gate.batch_ticks,
            gate.installer.as_ref().unwrap().next_continuity_counter()
        ),
        before
    );
    let (next, _, _) = deliver(&mut gate, 2);
    assert!(next.is_cycle_boundary());
}

#[test]
fn capped_prefix_preserves_post_terminator_source_and_all_absolute_positions() {
    let (mut gate, source) = gate(simple("nld", None), 12);
    let full = gate.window_end(4).unwrap().unwrap();
    let boundary = gate.transition_window_end(4).unwrap().unwrap();
    assert_eq!(boundary, u64::try_from(tests::end(3)).unwrap());
    assert!(boundary < full);
    assert!(gate
        .prepare_window(usize::try_from(boundary).unwrap(), Duration::ZERO)
        .unwrap());
    let schedule = gate.pending_schedule().unwrap();
    let at = schedule.last_due();
    let output = gate.pending_packets(schedule.first_due()).unwrap().to_vec();
    let receipt = gate.acknowledge_window_with_receipt(schedule, at).unwrap();
    assert!(receipt.is_cycle_boundary());
    assert_source_positions(&output, &source, 0);
    assert_eq!(gate.source_start, boundary);
    assert_eq!(
        gate.retained_packets().unwrap(),
        source.len() - usize::try_from(boundary).unwrap()
    );
    let (_, next, _) = deliver(&mut gate, 1);
    assert_source_positions(&next, &source, usize::try_from(boundary).unwrap());
}

#[test]
fn same_descriptor_replacement_shares_exact_model_and_keeps_clock_and_packetizer() {
    let (mut gate, source) = gate(simple("nld", None), 15);
    let (receipt, output, now) = cycle(&mut gate);
    let next = simple("nld", None);
    let ticks = gate.batch_ticks;
    let anchor = gate.anchor;
    let cc = gate.installer.as_ref().unwrap().next_continuity_counter();
    gate.replace_authored_at_acknowledged_boundary(&receipt, &next, now)
        .unwrap();
    assert!(!gate.completed_cycle);
    assert!(!gate.signalling_transition_pending());
    assert_eq!(gate.anchor, anchor);
    assert_eq!(gate.batch_ticks, ticks);
    assert_eq!(
        gate.installer.as_ref().unwrap().next_continuity_counter(),
        cc
    );
    assert!(Arc::ptr_eq(
        gate.carousel
            .as_ref()
            .unwrap()
            .authored_service_shared()
            .unwrap(),
        &next
    ));
    // The same accepted stamp cannot be reused merely because position is unchanged.
    assert_eq!(
        gate.withdraw_at_acknowledged_boundary(&receipt, now),
        Err(TeletextOutputError::Acknowledgement)
    );
    assert!(gate.failure.is_none());
    let (_, after, _) = cycle(&mut gate);
    assert_source_positions(&output, &source, 0);
    assert_source_positions(&after, &source, output.len());
    let authored: Vec<_> = output
        .iter()
        .chain(&after)
        .filter(|packet| pid(packet) == 512)
        .collect();
    for pair in authored.windows(2) {
        assert_eq!(pair[1][3] & 15, ((pair[0][3] & 15) + 1) & 15);
    }
}

#[test]
fn changed_descriptor_is_announced_and_acknowledged_before_any_new_pes() {
    let (mut gate, source) = gate(simple("nld", None), 15);
    let (receipt, old, now) = cycle(&mut gate);
    let first = gate.source_start;
    gate.replace_authored_at_acknowledged_boundary(&receipt, &simple("fra", None), now)
        .unwrap();
    assert!(gate.signalling_transition_pending());
    let end = gate.window_end(1).unwrap().unwrap();
    assert!(gate
        .prepare_window(usize::try_from(end - first).unwrap(), now)
        .unwrap());
    let schedule = gate.pending_schedule().unwrap();
    let bytes = gate.pending_packets(schedule.first_due()).unwrap().to_vec();
    assert!(!bytes.iter().any(|packet| pid(packet) == 512));
    let emitted = bytes.iter().find(|packet| pid(packet) == 3337).unwrap();
    let table = section(emitted);
    assert_eq!((table[5] >> 1) & 31, 2);
    assert_eq!(mpegts_crc::sum32(table), 0);
    assert!(table.windows(3).any(|bytes| bytes == b"fra"));
    assert_eq!(emitted[3] & 15, 1);
    assert!(gate.signalling_transition_pending());
    let acknowledged = gate
        .acknowledge_window_with_receipt(schedule, gate.due_at(end - 1).unwrap())
        .unwrap();
    assert!(acknowledged.signalling_announced());
    assert!(!acknowledged.contains_complete_cycle());
    assert!(!gate.signalling_transition_pending());
    assert!(!gate.completed_cycle);
    let (_, new, _) = cycle(&mut gate);
    assert!(new.iter().any(|packet| pid(packet) == 512));
    assert_source_positions(&bytes, &source, old.len());
    assert_source_positions(&new, &source, old.len() + bytes.len());
    let all: Vec<_> = old
        .iter()
        .chain(&new)
        .filter(|packet| pid(packet) == 512)
        .collect();
    for pair in all.windows(2) {
        assert_eq!(pair[1][3] & 15, ((pair[0][3] & 15) + 1) & 15);
    }
}

#[test]
fn withdrawal_and_reenable_keep_source_extent_pcr_and_continuity_without_cold_reset() {
    let old = simple("nld", None);
    let (mut gate, source) = gate(Arc::clone(&old), 18);
    let (receipt, mut all, now) = cycle(&mut gate);
    gate.withdraw_at_acknowledged_boundary(&receipt, now)
        .unwrap();
    assert!(!gate.authored_payload_selected());
    let first = all.len();
    let (withdrawn, bytes, now) = deliver(&mut gate, 1);
    assert!(withdrawn.signalling_announced());
    assert!(withdrawn.is_cycle_boundary());
    assert!(!bytes.iter().any(|packet| pid(packet) == 512));
    let table = section(bytes.iter().find(|packet| pid(packet) == 3337).unwrap());
    assert_eq!((table[5] >> 1) & 31, 2);
    assert!(!table.windows(3).any(|bytes| bytes == b"nld"));
    assert_eq!(mpegts_crc::sum32(table), 0);
    assert_source_positions(&bytes, &source, first);
    all.extend(bytes);
    gate.replace_authored_at_acknowledged_boundary(&withdrawn, &old, now)
        .unwrap();
    let (_, announced, _) = deliver(&mut gate, 1);
    let table = section(announced.iter().find(|packet| pid(packet) == 3337).unwrap());
    assert_eq!((table[5] >> 1) & 31, 3);
    assert!(!announced.iter().any(|packet| pid(packet) == 512));
    all.extend(announced);
    let (_, active, _) = cycle(&mut gate);
    all.extend(active);
    assert_source_positions(&all, &source, 0);
    let packets: Vec<_> = all.iter().filter(|packet| pid(packet) == 512).collect();
    for pair in packets.windows(2) {
        assert_eq!(pair[1][3] & 15, ((pair[0][3] & 15) + 1) & 15);
    }
}

#[test]
fn invalid_resource_lifecycle_preserves_current_content_signalling_and_next_output() {
    let old = Arc::new(candidate(spec(7, TeletextResourceFullUpdate::Changed)));
    let (mut gate, source) = gate(Arc::clone(&old), 100);
    let (receipt, bytes, now) = cycle(&mut gate);
    let snapshot = (
        gate.source_start,
        gate.batch_ticks,
        gate.installer.as_ref().unwrap().next_continuity_counter(),
    );
    let stale = Arc::new(candidate(spec(6, TeletextResourceFullUpdate::Unchanged)));
    assert!(matches!(
        gate.replace_authored_at_acknowledged_boundary(&receipt, &stale, now),
        Err(TeletextOutputError::Transport(
            TeletextTransportError::ResourceUpdate { .. }
        ))
    ));
    assert!(gate.failure.is_none());
    assert!(Arc::ptr_eq(
        gate.carousel
            .as_ref()
            .unwrap()
            .authored_service_shared()
            .unwrap(),
        &old
    ));
    assert_eq!(
        (
            gate.source_start,
            gate.batch_ticks,
            gate.installer.as_ref().unwrap().next_continuity_counter()
        ),
        snapshot
    );
    let (_, next, _) = cycle(&mut gate);
    assert_source_positions(&next, &source, bytes.len());
}

#[test]
fn retired_m29_and_mot_banks_require_reconciliation_without_damaging_existing_output() {
    for (old, next) in [
        (simple("nld", Some(1)), simple("nld", None)),
        (Arc::new(authored::candidate()), simple("nld", Some(1))),
    ] {
        let (mut gate, _) = gate(Arc::clone(&old), 100);
        let (receipt, _, now) = cycle(&mut gate);
        let cc = gate.installer.as_ref().unwrap().next_continuity_counter();
        assert_eq!(
            gate.replace_authored_at_acknowledged_boundary(&receipt, &next, now),
            Err(TeletextOutputError::CacheReconciliationRequired)
        );
        assert!(gate.failure.is_none());
        assert!(Arc::ptr_eq(
            gate.carousel
                .as_ref()
                .unwrap()
                .authored_service_shared()
                .unwrap(),
            &old
        ));
        assert_eq!(
            gate.installer.as_ref().unwrap().next_continuity_counter(),
            cc
        );
        let (repeated, _, _) = cycle(&mut gate);
        assert!(repeated.contains_complete_cycle());
    }
}

#[test]
fn pending_and_foreign_receipts_cannot_release_or_relabel_prepared_bytes() {
    let (mut first, _) = gate(simple("nld", None), 18);
    let (mut other, _) = gate(simple("nld", None), 18);
    let (foreign, _, _) = cycle(&mut other);
    let (receipt, _, now) = cycle(&mut first);
    assert_eq!(
        first.withdraw_at_acknowledged_boundary(&foreign, now),
        Err(TeletextOutputError::Acknowledgement)
    );
    assert!(first.failure.is_none());
    let end = first.window_end(1).unwrap().unwrap();
    assert!(first
        .prepare_window(usize::try_from(end - first.source_start).unwrap(), now)
        .unwrap());
    let schedule = first.pending_schedule().unwrap();
    let bytes = first
        .pending_packets(schedule.first_due())
        .unwrap()
        .to_vec();
    assert_eq!(
        first.withdraw_at_acknowledged_boundary(&receipt, schedule.first_due()),
        Err(TeletextOutputError::Pending)
    );
    assert_eq!(first.pending_packets(schedule.first_due()).unwrap(), bytes);
    assert!(first.failure.is_none());
    first
        .acknowledge_window(schedule, first.due_at(end - 1).unwrap())
        .unwrap();
    assert_eq!(
        first.withdraw_at_acknowledged_boundary(&receipt, first.due_at(end - 1).unwrap()),
        Err(TeletextOutputError::Acknowledgement)
    );
}

#[test]
fn eof_requires_acknowledged_withdrawal_and_new_replacement_cycle() {
    for finish_directly in [false, true] {
        let (mut gate, _) = gate(simple("nld", None), 3);
        let (receipt, _, now) = cycle(&mut gate);
        assert_eq!(
            gate.remaining_capacity().unwrap(),
            gate.config.max_buffer_packets * 188
        );
        gate.withdraw_at_acknowledged_boundary(&receipt, now)
            .unwrap();
        let result = if finish_directly {
            gate.finish(now).map(|()| false)
        } else {
            gate.prepare_final_window(now)
        };
        assert_eq!(result, Err(TeletextOutputError::Unterminated));
        assert!(gate.failure.is_some());
    }
    let (mut withdrawn, _) = gate(simple("nld", None), 4);
    let (receipt, _, now) = cycle(&mut withdrawn);
    withdrawn
        .withdraw_at_acknowledged_boundary(&receipt, now)
        .unwrap();
    let (announcement, _, now) = deliver(&mut withdrawn, 1);
    assert!(announcement.signalling_announced());
    assert!(!withdrawn.prepare_final_window(now).unwrap());
    assert_eq!(withdrawn.finish(now), Ok(()));

    let (mut replaced, _) = gate(simple("nld", None), 3);
    let (receipt, _, now) = cycle(&mut replaced);
    replaced
        .replace_authored_at_acknowledged_boundary(&receipt, &simple("nld", None), now)
        .unwrap();
    assert_eq!(
        replaced.prepare_final_window(now),
        Err(TeletextOutputError::Unterminated)
    );
}

fn checked_resource(counter: u8, bits: u16) -> Arc<EncodedTeletextAuthoredService> {
    let mut input = spec(counter, TeletextResourceFullUpdate::Changed);
    input.glyph = Some((0, bits));
    Arc::new(
        candidate(input)
            .with_resource_lifecycle_variants(std::time::Instant::now() + Duration::from_secs(30))
            .unwrap(),
    )
}

fn output_resource_header(
    packets: &[[u8; 188]],
) -> super::super::source_header::SuppliedTeletextHeader {
    use crate::teletext::source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyEvent};
    use crate::teletext::source_header::{observe_dvb_teletext_header, TeletextHeaderObservation};
    let mut assembler = SuppliedTeletextAssembler::new(512).unwrap();
    let mut result = None;
    for packet in packets {
        if let TeletextAssemblyEvent::Complete(pes) = assembler.push(packet).unwrap() {
            for unit in pes.units().filter_map(|unit| unit.teletext_unit()) {
                if let TeletextHeaderObservation::Header(header) =
                    observe_dvb_teletext_header(unit).unwrap()
                {
                    if header.wire_page == 0xc0 {
                        result.get_or_insert(header);
                    }
                }
            }
        }
    }
    assembler.finish().unwrap();
    result.expect("completed output cycle must emit the exact DRCS resource header")
}

#[test]
fn checked_resource_variant_changes_only_after_ack_and_repeat_keeps_effective_counter() {
    let mut input = spec(7, TeletextResourceFullUpdate::Changed);
    input.glyph = Some((0, 1));
    let old = Arc::new(candidate(input));
    let (mut gate, source) = gate(Arc::clone(&old), 200);
    let (receipt, before, now) = cycle(&mut gate);
    let next = checked_resource(7, 2);
    let baseline: Vec<_> = next
        .pages()
        .iter()
        .map(|page| page.data_units().to_vec())
        .collect();
    let anchor = gate.anchor;
    let ticks = gate.batch_ticks;
    let continuity = gate.installer.as_ref().unwrap().next_continuity_counter();
    gate.replace_authored_at_acknowledged_boundary(&receipt, &next, now)
        .unwrap();
    assert_eq!(gate.anchor, anchor);
    assert_eq!(gate.batch_ticks, ticks);
    assert_eq!(
        gate.installer.as_ref().unwrap().next_continuity_counter(),
        continuity
    );
    assert!(!gate.signalling_transition_pending());
    assert!(Arc::ptr_eq(
        gate.carousel
            .as_ref()
            .unwrap()
            .authored_service_shared()
            .unwrap(),
        &next
    ));
    let (_, first, _) = cycle(&mut gate);
    let first_header = output_resource_header(&first);
    assert_eq!((first_header.subcode >> 4) & 7, 0);
    assert!(first_header.controls.erase_page());
    assert_ne!(first_header.controls.raw() & 16, 0);
    let (_, repeat, _) = cycle(&mut gate);
    let repeat_header = output_resource_header(&repeat);
    assert_eq!((repeat_header.subcode >> 4) & 7, 0);
    assert!(!repeat_header.controls.erase_page());
    assert_eq!(repeat_header.controls.raw() & 16, 0);
    assert_source_positions(&before, &source, 0);
    assert_source_positions(&first, &source, before.len());
    assert_source_positions(&repeat, &source, before.len() + first.len());
    assert_eq!(
        next.pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        baseline
    );
    let authored: Vec<_> = before
        .iter()
        .chain(&first)
        .chain(&repeat)
        .filter(|packet| pid(packet) == 512)
        .collect();
    for pair in authored.windows(2) {
        assert_eq!(pair[1][3] & 15, ((pair[0][3] & 15) + 1) & 15);
    }
}

#[test]
fn checked_resource_variant_pending_and_foreign_acks_leave_plan_and_prepared_bytes_untouched() {
    let old = Arc::new(candidate(spec(7, TeletextResourceFullUpdate::Changed)));
    let (mut first, _) = gate(Arc::clone(&old), 200);
    let (mut other, _) = gate(Arc::clone(&old), 200);
    let (foreign, _, _) = cycle(&mut other);
    let (receipt, _, now) = cycle(&mut first);
    let next = checked_resource(7, 2);
    let snapshot = (
        first.source_start,
        first.batch_ticks,
        first.installer.as_ref().unwrap().next_continuity_counter(),
    );
    assert_eq!(
        first.replace_authored_at_acknowledged_boundary(&foreign, &next, now),
        Err(TeletextOutputError::Acknowledgement)
    );
    assert_eq!(
        (
            first.source_start,
            first.batch_ticks,
            first.installer.as_ref().unwrap().next_continuity_counter()
        ),
        snapshot
    );
    assert!(Arc::ptr_eq(
        first
            .carousel
            .as_ref()
            .unwrap()
            .authored_service_shared()
            .unwrap(),
        &old
    ));
    let end = first.window_end(1).unwrap().unwrap();
    assert!(first
        .prepare_window(usize::try_from(end - first.source_start).unwrap(), now)
        .unwrap());
    let schedule = first.pending_schedule().unwrap();
    let pending = first
        .pending_packets(schedule.first_due())
        .unwrap()
        .to_vec();
    assert_eq!(
        first.replace_authored_at_acknowledged_boundary(&receipt, &next, schedule.first_due()),
        Err(TeletextOutputError::Pending)
    );
    assert_eq!(
        first.pending_packets(schedule.first_due()).unwrap(),
        pending
    );
    assert!(Arc::ptr_eq(
        first
            .carousel
            .as_ref()
            .unwrap()
            .authored_service_shared()
            .unwrap(),
        &old
    ));
    first
        .acknowledge_window(schedule, first.due_at(end - 1).unwrap())
        .unwrap();
    assert_eq!(
        first.replace_authored_at_acknowledged_boundary(
            &receipt,
            &next,
            first.due_at(end - 1).unwrap()
        ),
        Err(TeletextOutputError::Acknowledgement)
    );
    assert!(first.failure.is_none());
    let (valid, _, now) = cycle(&mut first);
    first
        .replace_authored_at_acknowledged_boundary(&valid, &next, now)
        .unwrap();
    let (_, bytes, _) = cycle(&mut first);
    // Neither rejected proposal selected or completed a resource update.
    let header = output_resource_header(&bytes);
    assert_eq!((header.subcode >> 4) & 7, 0);
    assert!(header.controls.erase_page());
}
