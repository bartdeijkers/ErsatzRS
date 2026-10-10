use super::*;
use crate::transport::iptv::teletext_output::tests::{authored_set, policy};

const ROLES: [TeletextOutputRole; 6] = [
    TeletextOutputRole::FinalDirectMpegTs,
    TeletextOutputRole::FinalWrapperMpegTs,
    TeletextOutputRole::InternalItem,
    TeletextOutputRole::HlsWorker,
    TeletextOutputRole::OtherContainer,
    TeletextOutputRole::CapacitySlate,
];

const FORMATS: [OutputFormat; 6] = [
    OutputFormat::MpegTs,
    OutputFormat::Hls,
    OutputFormat::HlsMp4,
    OutputFormat::Mp4,
    OutputFormat::Mkv,
    OutputFormat::Nut,
];

fn binding() -> Arc<TeletextOutputBinding> {
    Arc::new(
        TeletextOutputBinding::new(policy(), authored_set(), Duration::from_millis(40)).unwrap(),
    )
}

#[test]
fn plain_process_roles_require_their_actual_generated_container() {
    for role in ROLES {
        let admission = TeletextOutputAdmission::plain(role);
        assert_eq!(admission.role(), role);
        assert!(admission.binding().is_none());
        for format in FORMATS {
            let accepted = match role {
                TeletextOutputRole::HlsWorker => {
                    matches!(format, OutputFormat::Hls | OutputFormat::HlsMp4)
                }
                TeletextOutputRole::OtherContainer => format != OutputFormat::MpegTs,
                _ => format == OutputFormat::MpegTs,
            };
            assert_eq!(admission.validate_format(format).is_ok(), accepted);
        }
    }
}

#[test]
fn authored_pages_are_admitted_only_on_final_direct_or_wrapper_transport_outputs() {
    for role in ROLES {
        let result = TeletextOutputAdmission::with_binding(role, binding());
        if matches!(
            role,
            TeletextOutputRole::FinalDirectMpegTs | TeletextOutputRole::FinalWrapperMpegTs
        ) {
            let admission = result.unwrap();
            for format in FORMATS {
                assert_eq!(
                    admission.validate_format(format).is_ok(),
                    format == OutputFormat::MpegTs
                );
            }
        } else {
            assert!(matches!(result, Err(TeletextPumpError::Configuration)));
        }
    }
}

#[test]
fn cloning_admission_preserves_the_same_immutable_binding_and_caller_policy() {
    let binding = binding();
    let pages = binding.pages().clone();
    let admission = TeletextOutputAdmission::with_binding(
        TeletextOutputRole::FinalDirectMpegTs,
        Arc::clone(&binding),
    )
    .unwrap();
    let cloned = admission.clone();
    let original = admission.binding().unwrap();
    assert!(std::ptr::eq(original, cloned.binding().unwrap()));
    assert!(std::ptr::eq(original, binding.as_ref()));
    assert_eq!(original.pages(), &pages);
    assert_eq!(original.config().program, policy().program);
    assert_eq!(original.config().elementary_pid, policy().elementary_pid);
    assert_eq!(original.config().max_hold, policy().max_hold);
    assert_eq!(original.first_byte_timeout(), Duration::from_millis(40));
}

#[test]
fn invalid_gate_policy_cannot_be_wrapped_in_an_admissible_binding() {
    for kind in 0..3 {
        let mut config = policy();
        match kind {
            0 => config.max_buffer_packets = 0,
            1 => config.playout_delay = Duration::ZERO,
            _ => config.clock.muxrate_bps = 0,
        }
        assert!(matches!(
            TeletextOutputBinding::new(config, authored_set(), Duration::from_millis(40)),
            Err(TeletextPumpError::Gate(_))
        ));
    }
}

#[test]
fn invalid_async_timing_cannot_be_wrapped_in_a_valid_transport_policy() {
    for timeout in [Duration::ZERO, Duration::MAX] {
        assert!(matches!(
            TeletextOutputBinding::new(policy(), authored_set(), timeout),
            Err(TeletextPumpError::Configuration)
        ));
    }
    let mut config = policy();
    config.max_hold = Duration::MAX;
    assert!(matches!(
        TeletextOutputBinding::new(config, authored_set(), Duration::from_millis(40)),
        Err(TeletextPumpError::Configuration)
    ));
}
