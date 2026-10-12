use super::{TeletextMultiplexError, TeletextPmtPlan, TeletextServiceDescriptor};
use crate::teletext::{
    psi_tests::{pmt, revise},
    signalling_tests::{page, pages},
};
use mpeg2ts_reader::mpegts_crc;

fn descriptor(number: u16) -> TeletextServiceDescriptor {
    TeletextServiceDescriptor::from_page_set(&pages(vec![page(number, 0, "eng", false)]))
}

#[test]
fn output_versions_wrap_and_advance_across_withdrawal_and_readdition() {
    let source = revise(pmt(10, 300, 300, &[]), |section| {
        section[5] = 0xc1 | (30 << 1);
    });
    let first = TeletextPmtPlan::new(&source, 3337, 512, &descriptor(200)).unwrap();
    assert_eq!((first.section()[5] >> 1) & 31, 31);
    let second = first.reconfigure(Some(&descriptor(201))).unwrap();
    let third = second.reconfigure(None).unwrap();
    let fourth = third.reconfigure(Some(&descriptor(202))).unwrap();
    for (plan, version) in [(&second, 0), (&third, 1), (&fourth, 2)] {
        assert_eq!((plan.section()[5] >> 1) & 31, version);
        assert_eq!(mpegts_crc::sum32(plan.section()), 0);
        assert_eq!(plan.source_section(), source);
        assert_eq!(plan.program_number(), 10);
        assert_eq!(plan.pmt_pid(), 3337);
        assert_eq!(plan.pcr_pid(), 300);
        assert_eq!(plan.elementary_pid(), 512);
    }
    assert!(second.is_reconfiguration_of(&first));
    assert!(!fourth.is_reconfiguration_of(&first));
}

#[test]
fn withdrawn_table_preserves_exact_original_program_and_stream_descriptor_bytes() {
    // Caption and unknown descriptors belong to the source, not the authored PID.
    let source = pmt(10, 300, 300, &[0x86, 3, 0xc1, 0, 0]);
    let first = TeletextPmtPlan::new(&source, 3337, 512, &descriptor(200)).unwrap();
    let withdrawn = first.reconfigure(None).unwrap();
    let expected = revise(source.clone(), |section| {
        section[5] = 0xc1 | (2 << 1);
    });
    assert_eq!(withdrawn.section(), expected);
    assert_eq!(withdrawn.source_section(), source);
    assert_eq!(withdrawn.elementary_pid(), 512);
}

#[test]
fn reconfiguration_capacity_rejection_preserves_original_plan_and_exact_fit_works() {
    let original =
        TeletextPmtPlan::new(&pmt(10, 300, 300, &[]), 3337, 512, &descriptor(200)).unwrap();
    let before = original.clone();
    for bytes in [Vec::new(), vec![0; 1_024]] {
        assert_eq!(
            original.reconfigure(Some(&TeletextServiceDescriptor { bytes })),
            Err(TeletextMultiplexError::SectionCapacity)
        );
        assert_eq!(original, before);
    }
    let size = 1_024 - original.source_section().len() - 5;
    let result = original
        .reconfigure(Some(&TeletextServiceDescriptor {
            bytes: vec![0; size],
        }))
        .unwrap();
    assert_eq!(result.section().len(), 1_024);
    assert_eq!(mpegts_crc::sum32(result.section()), 0);
}
