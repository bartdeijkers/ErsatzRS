//! Optional page observations owned by the same single native producer.
//!
//! Bounded prefix evidence is neither a complete inventory nor a local-file or
//! clock receipt. Unknown captions/metadata preserve this owner's ordinary AV.

use std::{collections::BTreeSet, fmt, time::Duration};

use ersatzrs_domain::TeletextPageNumber;
use ersatzrs_ffmpeg::{
    teletext::{
        source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyEvent},
        source_descriptors::{parse_source_teletext_descriptors, SourceTeletextWireDescriptor},
        source_observation::SuppliedTeletextHeaderObserver,
        source_psi::{ObservedSourcePsi, SourceTeletextPsiObserver},
    },
    FfmpegCommand, InputFile, SourceAudioStream, SourceTeletextInputPlan, SourceVideoStream,
};
use tokio::{sync::oneshot, time::Instant};

use super::{
    PreparedRemoteStreamInput, RemoteStreamCommand, RemoteStreamError, RemoteStreamSpawnContext,
};
use crate::{
    ffprobe::{
        teletext_probe::{
            ProbedTeletextStream, TeletextProbeDescriptors, TeletextProbePid,
            TeletextProbePrograms, TeletextProbeSnapshot,
        },
        FfprobeError, FfprobeExtractor, FfprobeMediaVersion,
    },
    playback::{
        source_audio_streams_from_probe, source_video_stream_from_probe,
        transient::TransientPlaybackTracks, PlaybackSubtitle,
    },
};

const MAX_CHOICES: usize = 1_024;

/// One playback-owned producer, optional fresh metadata and qualified choices.
/// `probe == None` means unknown source inventory, including unknown audio;
/// an empty `source_audio` must then never become automatic silence authority.
pub struct PreparedNativeCaptionPlayback {
    pub prepared: PreparedRemoteStreamInput,
    pub video_input: InputFile,
    /// Copied-prefix metadata; its duration is never whole-source/EOF authority.
    pub probe: Option<FfprobeMediaVersion>,
    pub source_video: Option<SourceVideoStream>,
    pub source_audio: Vec<SourceAudioStream>,
    tracks: TransientPlaybackTracks,
    choices: Vec<CaptionChoice>,
}

impl fmt::Debug for PreparedNativeCaptionPlayback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreparedNativeCaptionPlayback(<owned>)")
    }
}

#[derive(Clone)]
struct CaptionChoice {
    subtitle: PlaybackSubtitle,
    pid: u16,
    program: u16,
    page: TeletextPageNumber,
    role: u8,
    observed: bool,
}

pub(super) struct NativeCaptionBinding {
    selected: Option<InputFile>,
    command: Option<FfmpegCommand>,
    video: Option<usize>,
    audio: BTreeSet<usize>,
}

impl NativeCaptionBinding {
    pub(super) fn matches(&self, command: &FfmpegCommand) -> bool {
        self.command.as_ref() == Some(command)
    }

    fn validate(&self, command: &FfmpegCommand, plan: Option<&SourceTeletextInputPlan>) -> bool {
        if !uses_owned_input(command) {
            return false;
        }
        let Some(selected) = &self.selected else {
            return plan.is_none();
        };
        let Some(plan) = plan else { return false };
        let timing = plan.first_input_timing();
        plan.matches_command(command)
            && plan.caption_input() == selected
            && plan.caption_physical_input_index() == 0
            && plan.video_stream() == self.video.map(|index| (0, index))
            && self.video.is_some()
            && plan
                .audio_stream()
                .is_none_or(|(owner, index)| owner == 0 && self.audio.contains(&index))
            && timing.effective_input_seek_microseconds() == Some(0)
            && timing.output_seek().is_none_or(|seek| seek.is_zero())
            && !timing.retained_stream_timestamps()
            && !timing.has_opaque_input_options()
            && !plan.is_bitmap_carriage()
    }
}

// The ordinary no-caption builder has no public physical-input plan. Its
// existing typed request remains caller authority. Reuse the literal generated
// -i convention from remote_stream::take_remote_stream_output_seek, without
// interpreting opaque options, filter graphs or arbitrary map syntax.
fn uses_owned_input(command: &FfmpegCommand) -> bool {
    command
        .arguments
        .windows(2)
        .find(|pair| pair[0] == "-i")
        .is_some_and(|pair| pair[1] == "pipe:0")
        && command
            .arguments
            .windows(2)
            .filter(|pair| pair[0] == "-i" && pair[1] == "pipe:0")
            .count()
            == 1
}

impl PreparedNativeCaptionPlayback {
    /// Fresh ordinary bitmap captions plus unambiguous observed source pages.
    /// Raw embedded text/teletext rows never escape as playable alternatives.
    #[must_use]
    pub fn tracks(&self) -> TransientPlaybackTracks {
        self.tracks.clone()
    }

    /// Select only an exact choice from this producer's original track values.
    /// None lets the caller use its existing ordinary bitmap input helper.
    /// # Errors
    /// Rejects fabricated, ambiguous or already sealed choices.
    pub fn subtitle_input(
        &mut self,
        subtitle: &PlaybackSubtitle,
    ) -> Result<Option<InputFile>, RemoteStreamError> {
        let binding = self
            .prepared
            .native_caption_binding
            .as_mut()
            .ok_or_else(unavailable)?;
        if binding.command.is_some() || !self.tracks.subtitles.contains(subtitle) {
            return Err(unavailable());
        }
        let input = selected_input(&self.choices, subtitle)?;
        binding.selected.clone_from(&input);
        Ok(input)
    }

    /// Seal the final generated shared-pipe consumer before transferring ownership.
    /// The live page branch admits no independent source seek/open or local
    /// initializer. Programme duration remains the existing caller's policy.
    /// # Errors
    /// Rejects changed selection, command, physical owners or timing policy.
    pub fn bind_caption_consumer(
        &mut self,
        command: &FfmpegCommand,
        plan: Option<&SourceTeletextInputPlan>,
    ) -> Result<(), RemoteStreamError> {
        let binding = self
            .prepared
            .native_caption_binding
            .as_mut()
            .ok_or_else(unavailable)?;
        if !binding.validate(command, plan)
            || binding
                .command
                .as_ref()
                .is_some_and(|sealed| sealed != command)
        {
            return Err(unavailable());
        }
        binding.command = Some(command.clone());
        Ok(())
    }
}

fn unavailable() -> RemoteStreamError {
    RemoteStreamError::PreparedCaptionUnavailable
}

fn selected_input(
    choices: &[CaptionChoice],
    subtitle: &PlaybackSubtitle,
) -> Result<Option<InputFile>, RemoteStreamError> {
    let matches = choices
        .iter()
        .filter(|choice| choice.subtitle == *subtitle)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] if subtitle.codec.as_deref() != Some("dvb_teletext") => Ok(None),
        [choice] => Ok(Some(InputFile::teletext_subtitle(
            "pipe:0",
            usize::try_from(subtitle.stream_index).map_err(|_| unavailable())?,
            choice.page,
        ))),
        _ => Err(unavailable()),
    }
}

fn check(
    deadline: Instant,
    cancellation: &mut oneshot::Receiver<()>,
) -> Result<(), RemoteStreamError> {
    if !matches!(
        cancellation.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ) {
        Err(RemoteStreamError::PreparedCancelled)
    } else if Instant::now() >= deadline {
        Err(RemoteStreamError::PreparedTimeout)
    } else {
        Ok(())
    }
}

/// Start once, observe only copied bounded bytes and retain the original owner.
/// First output admits startup; optional caption failure never causes a second
/// source open. Additional prefix reads consume only immediately available bytes.
/// # Errors
/// Cancellation, expired startup, producer failure and uncertain cleanup are
/// fatal and await producer cleanup; unavailable optional metadata is not fatal.
pub async fn prepare_native_caption_producer(
    command: &RemoteStreamCommand,
    extractor: &FfprobeExtractor,
    context: RemoteStreamSpawnContext,
    deadline: Instant,
    mut cancellation: oneshot::Receiver<()>,
) -> Result<PreparedNativeCaptionPlayback, RemoteStreamError> {
    let deadline = deadline.min(Instant::now() + Duration::from_secs(15));
    check(deadline, &mut cancellation)?;
    let (cancel, receiver) = oneshot::channel();
    let start =
        PreparedRemoteStreamInput::start_caption_prefix(command, context, deadline, receiver);
    tokio::pin!(start);
    let mut prepared = tokio::select! {
        biased;
        _ = &mut cancellation => {
            drop(cancel);
            match start.await {
                Ok(prepared) => prepared.cancel().await?,
                Err(RemoteStreamError::PreparedTerminationUnconfirmed) =>
                    return Err(RemoteStreamError::PreparedTerminationUnconfirmed),
                Err(_) => (),
            }
            return Err(RemoteStreamError::PreparedCancelled);
        },
        result = &mut start => result?,
    };
    let observation = observe(&mut prepared, extractor, deadline, &mut cancellation).await;
    let observation = match observation {
        Ok(observation) => observation,
        Err(error) => {
            prepared.cancel().await?;
            return Err(error);
        }
    };
    let (probe, source_video, source_audio, choices) = observation;
    let mut tracks = probe
        .as_ref()
        .map_or_else(TransientPlaybackTracks::default, |probe| {
            TransientPlaybackTracks::from_streams(&probe.streams)
        });
    tracks.subtitles.retain(PlaybackSubtitle::is_image);
    tracks
        .subtitles
        .extend(choices.iter().map(|choice| choice.subtitle.clone()));
    prepared.native_caption_binding = Some(Box::new(NativeCaptionBinding {
        selected: None,
        command: None,
        video: source_video.as_ref().map(|video| video.index),
        audio: source_audio.iter().map(|audio| audio.index).collect(),
    }));
    Ok(PreparedNativeCaptionPlayback {
        prepared,
        video_input: InputFile::video("pipe:0").with_seek_after_input(),
        probe,
        source_video,
        source_audio,
        tracks,
        choices,
    })
}

type Observation = (
    Option<FfprobeMediaVersion>,
    Option<SourceVideoStream>,
    Vec<SourceAudioStream>,
    Vec<CaptionChoice>,
);

async fn observe(
    prepared: &mut PreparedRemoteStreamInput,
    extractor: &FfprobeExtractor,
    deadline: Instant,
    cancellation: &mut oneshot::Receiver<()>,
) -> Result<Observation, RemoteStreamError> {
    let mut best = (None, None, Vec::new(), Vec::new());
    loop {
        check(deadline, cancellation)?;
        let observed = {
            let (cancel, receiver) = oneshot::channel();
            let probe = extractor.extract_producer_prefix_with_teletext_cancellable(
                prepared.prefix(),
                deadline,
                receiver,
            );
            tokio::pin!(probe);
            tokio::select! {
                biased;
                _ = &mut *cancellation => {
                    drop(cancel);
                    if matches!(probe.await, Err(FfprobeError::CleanupUnconfirmed { .. })) {
                        return Err(RemoteStreamError::PreparedTerminationUnconfirmed);
                    }
                    return Err(RemoteStreamError::PreparedCancelled);
                },
                result = &mut probe => result,
            }
        };
        match observed {
            Ok((media, snapshot)) => {
                if let (Ok(Some(video)), Ok(audio)) = (
                    source_video_stream_from_probe(&media),
                    source_audio_streams_from_probe(&media),
                ) {
                    let choices = qualify(prepared.prefix(), &media, snapshot).unwrap_or_default();
                    best = (Some(media), Some(video), audio, choices);
                }
            }
            Err(FfprobeError::CleanupUnconfirmed { .. }) => {
                return Err(RemoteStreamError::PreparedTerminationUnconfirmed)
            }
            Err(FfprobeError::Cancelled { .. }) => {
                return Err(RemoteStreamError::PreparedCancelled)
            }
            Err(FfprobeError::Timeout { .. }) => return Err(RemoteStreamError::PreparedTimeout),
            Err(_) => (), // The same producer remains available to ordinary AV.
        }
        check(deadline, cancellation)?;
        prepared.check_caption_producer_status()?;
        if !best.3.is_empty() || !prepared.extend_caption_prefix()? {
            return Ok(best);
        }
    }
}

fn observe_wire(
    prefix: &[u8],
    pid: u16,
) -> Result<(ObservedSourcePsi, BTreeSet<TeletextPageNumber>), RemoteStreamError> {
    let mut psi = SourceTeletextPsiObserver::new_source_epoch(pid).map_err(|_| unavailable())?;
    let mut pes = SuppliedTeletextAssembler::new_source_epoch(pid).map_err(|_| unavailable())?;
    let mut complete_end = None;
    for (index, raw) in prefix.chunks_exact(188).enumerate() {
        let raw: &[u8; 188] = raw.try_into().map_err(|_| unavailable())?;
        psi.push(raw).map_err(|_| unavailable())?;
        let selected = ((u16::from(raw[1] & 0x1f) << 8) | u16::from(raw[2])) == pid;
        if selected && raw[3] & 0x10 != 0 && !psi.has_complete_current_tables() {
            return Err(unavailable());
        }
        if matches!(
            pes.push(raw).map_err(|_| unavailable())?,
            TeletextAssemblyEvent::Complete(_)
        ) {
            complete_end = Some((index + 1) * 188);
        }
    }
    let psi = psi.observation().map_err(|_| unavailable())?;
    if psi.current_tables_changed {
        return Err(unavailable());
    }
    let Some(end) = complete_end else {
        return Ok((psi, BTreeSet::new()));
    };
    let mut headers = SuppliedTeletextHeaderObserver::new_source_epoch_with_pages(pid)
        .map_err(|_| unavailable())?;
    for raw in prefix[..end].chunks_exact(188) {
        headers
            .push(raw.try_into().map_err(|_| unavailable())?)
            .map_err(|_| unavailable())?;
    }
    let headers = headers.finish().map_err(|_| unavailable())?;
    if headers.has_unobserved_prefix() {
        return Err(unavailable());
    }
    Ok((
        psi,
        headers
            .headers()
            .iter()
            .filter_map(|entry| entry.header.display_page())
            .collect(),
    ))
}

fn qualify(
    prefix: &[u8],
    media: &FfprobeMediaVersion,
    snapshot: TeletextProbeSnapshot,
) -> Result<Vec<CaptionChoice>, RemoteStreamError> {
    let TeletextProbeSnapshot::Available(streams) = snapshot else {
        return Ok(Vec::new());
    };
    let tracks = TransientPlaybackTracks::from_streams(&media.streams);
    let mut choices = Vec::new();
    for stream in streams {
        let rows = tracks
            .subtitles
            .iter()
            .filter(|row| {
                row.stream_index == stream.stream_index
                    && row.codec.as_deref() == Some("dvb_teletext")
            })
            .collect::<Vec<_>>();
        if let [row] = rows.as_slice() {
            append_choices(prefix, &stream, row, &mut choices)?;
        }
    }
    Ok(unambiguous(&choices))
}

fn append_choices(
    prefix: &[u8],
    stream: &ProbedTeletextStream,
    row: &PlaybackSubtitle,
    choices: &mut Vec<CaptionChoice>,
) -> Result<(), RemoteStreamError> {
    let (
        TeletextProbePid::Available(pid),
        TeletextProbePrograms::Available(programs),
        TeletextProbeDescriptors::Available(projected),
    ) = (&stream.pid, &stream.programs, &stream.descriptors)
    else {
        return Ok(());
    };
    let (psi, headers) = observe_wire(prefix, *pid)?;
    let members = psi.memberships().collect::<Vec<_>>();
    if members.len() != programs.len()
        || members.iter().any(|(program, _)| {
            !programs.iter().any(|reported| {
                u32::from(program.program_number) == reported.program_number
                    && program.pmt_pid == reported.pmt_pid
                    && program.pcr_pid == reported.pcr_pid
            })
        })
    {
        return Err(unavailable());
    }
    for (program, elementary) in members {
        if elementary.stream_type != 6 {
            continue;
        }
        let entries = parse_source_teletext_descriptors(&elementary.descriptors)
            .map_err(|_| unavailable())?;
        if !entries.iter().any(|entry| entry.descriptor_tag == 0x56) {
            continue;
        }
        for entry in entries {
            let Some(page) = entry
                .display_page()
                .filter(|_| matches!(entry.descriptor_type, 2 | 5))
            else {
                continue;
            };
            if !projected.entries.iter().any(|fact| {
                fact.language == entry.language
                    && fact.descriptor_type == entry.descriptor_type
                    && fact.wire_magazine == entry.wire_magazine
                    && fact.wire_page == entry.wire_page
            }) {
                return Err(unavailable());
            }
            if choices.len() == MAX_CHOICES {
                return Err(unavailable());
            }
            choices.push(choice(
                row,
                *pid,
                program.program_number,
                page,
                &entry,
                headers.contains(&page),
            ));
        }
    }
    Ok(())
}

fn choice(
    row: &PlaybackSubtitle,
    pid: u16,
    program: u16,
    page: TeletextPageNumber,
    descriptor: &SourceTeletextWireDescriptor,
    observed: bool,
) -> CaptionChoice {
    let mut subtitle = row.clone();
    subtitle.language = Some(descriptor.language.as_str().to_owned());
    subtitle.sdh |= descriptor.descriptor_type == 5;
    CaptionChoice {
        subtitle,
        pid,
        program,
        page,
        role: descriptor.descriptor_type,
        observed,
    }
}

fn same_language(left: &CaptionChoice, right: &CaptionChoice) -> bool {
    let Some(language) = left.subtitle.language.as_deref() else {
        return false;
    };
    let codes = ersatzrs_core::language_codes::all_language_codes([language]);
    right
        .subtitle
        .language
        .as_deref()
        .is_some_and(|language| codes.iter().any(|code| code.eq_ignore_ascii_case(language)))
}

fn same_identity(left: &CaptionChoice, right: &CaptionChoice) -> bool {
    left.subtitle.stream_index == right.subtitle.stream_index
        && left.pid == right.pid
        && left.program == right.program
        && left.page == right.page
        && left.role == right.role
}

fn unambiguous(choices: &[CaptionChoice]) -> Vec<CaptionChoice> {
    choices
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            candidate.observed
                && !choices.iter().any(|other| {
                    other.subtitle.stream_index == candidate.subtitle.stream_index
                        && same_language(candidate, other)
                        && !same_identity(candidate, other)
                })
                && !choices[..*index]
                    .iter()
                    .any(|prior| same_language(candidate, prior) && same_identity(candidate, prior))
        })
        .map(|(_, choice)| choice.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{HardwareAccelerationKind, TeletextLanguage};
    use ersatzrs_ffmpeg::{DesiredVideoState, PipelineRequest, SourceAudioCodec, SourceVideoCodec};

    fn row() -> PlaybackSubtitle {
        PlaybackSubtitle {
            id: 0,
            subtitle_kind: crate::metadata::subtitle_kind::EMBEDDED,
            title: Some("Original title".into()),
            stream_index: 2,
            codec: Some("dvb_teletext".into()),
            default: true,
            forced: true,
            sdh: false,
            language: Some("eng,nld".into()),
            is_extracted: false,
            path: None,
        }
    }

    fn candidate(page: u16, language: &str, role: u8, observed: bool) -> CaptionChoice {
        let page = TeletextPageNumber::new(page).unwrap();
        let descriptor = SourceTeletextWireDescriptor {
            descriptor_tag: 0x56,
            descriptor_ordinal: 0,
            entry_ordinal: 0,
            language: TeletextLanguage::new(language.into()).unwrap(),
            descriptor_type: role,
            wire_magazine: 0,
            wire_page: 0x88,
        };
        choice(&row(), 0x102, 1, page, &descriptor, observed)
    }

    #[test]
    fn observed_choices_preserve_original_flags_aliases_and_affirmative_sdh() {
        let choices = unambiguous(&[
            candidate(888, "nld", 5, true),
            candidate(888, "dut", 5, true),
        ]);
        assert_eq!(
            choices.len(),
            1,
            "equivalent aliases do not create false ambiguity"
        );
        let chosen = &choices[0].subtitle;
        assert_eq!(chosen.title, row().title);
        assert!(chosen.default && chosen.forced && chosen.sdh);
        assert_eq!(chosen.language.as_deref(), Some("nld"));
        assert!(!chosen.is_extracted);
        let input = selected_input(&choices, chosen).unwrap().unwrap();
        assert_eq!(input.path, "pipe:0");
        assert_eq!(
            input.streams[0].source_teletext_page,
            Some(TeletextPageNumber::new(888).unwrap())
        );
    }

    #[test]
    fn descriptors_alone_and_conflicting_page_role_or_program_never_select() {
        assert!(unambiguous(&[candidate(888, "eng", 2, false)]).is_empty());
        for mut conflicting in [
            candidate(889, "eng", 2, false),
            candidate(888, "eng", 5, true),
            candidate(888, "eng", 2, true),
        ] {
            if conflicting.page == TeletextPageNumber::new(888).unwrap() && conflicting.role == 2 {
                conflicting.program = 2;
            }
            assert!(unambiguous(&[candidate(888, "eng", 2, true), conflicting]).is_empty());
        }
        assert_eq!(
            unambiguous(&[
                candidate(888, "eng", 2, true),
                candidate(889, "nld", 2, true)
            ])
            .len(),
            2
        );
    }

    #[test]
    fn fabricated_selected_teletext_metadata_is_not_a_page_selector() {
        let choices = vec![candidate(888, "eng", 2, true)];
        let mut fake = choices[0].subtitle.clone();
        fake.forced = !fake.forced;
        assert!(selected_input(&choices, &fake).is_err());
        fake = choices[0].subtitle.clone();
        fake.stream_index = 3;
        assert!(selected_input(&choices, &fake).is_err());
        assert!(selected_input(&[], &row()).is_err());
    }

    fn request(page: u16) -> PipelineRequest {
        let mut request = PipelineRequest::new(
            InputFile::video("pipe:0").with_seek_after_input(),
            DesiredVideoState::h264_1080p(),
        )
        .with_source_video_stream(
            SourceVideoStream::new(SourceVideoCodec::Mpeg2Video).with_index(0),
        )
        .with_source_audio_streams(vec![
            SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3)
        ]);
        request.inputs[1] = request.inputs[1].clone().with_stream_index(3);
        request.inputs.push(InputFile::teletext_subtitle(
            "pipe:0",
            2,
            TeletextPageNumber::new(page).unwrap(),
        ));
        request
    }

    fn binding() -> NativeCaptionBinding {
        NativeCaptionBinding {
            selected: Some(InputFile::teletext_subtitle(
                "pipe:0",
                2,
                TeletextPageNumber::new(888).unwrap(),
            )),
            command: None,
            video: Some(0),
            audio: BTreeSet::from([3]),
        }
    }

    #[test]
    fn exact_generated_shared_pipe_command_and_complete_seal_are_mandatory() {
        let (command, plan) = ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
            HardwareAccelerationKind::None,
            request(888),
        )
        .unwrap();
        insta::assert_json_snapshot!("native_caption_shared_pipe_argv", &command.arguments);
        let mut binding = binding();
        assert!(binding.validate(&command, plan.as_ref()));
        assert!(!binding.matches(&command), "unbound owner cannot spawn");
        binding.command = Some(command.clone());
        assert!(binding.matches(&command));
        let mut changed = command.clone();
        changed.arguments.push("-nostdin".into());
        assert!(!binding.matches(&changed));
        assert!(!binding.validate(&changed, plan.as_ref()));
        assert!(!binding.validate(&command, None));
        let (wrong, wrong_plan) =
            ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
                HardwareAccelerationKind::None,
                request(200),
            )
            .unwrap();
        assert!(!binding.validate(&wrong, wrong_plan.as_ref()));
    }

    #[test]
    fn independent_audio_seek_and_unknown_input_policy_are_refused() {
        let mut independent = request(888);
        independent.inputs[1].path = "other-input".into();
        let mut sought = request(888);
        sought.state.start = Some(Duration::from_secs(1));
        let mut opaque = request(888);
        opaque.inputs[0] = opaque.inputs[0]
            .clone()
            .with_input_options(["-f", "mpegts"]);
        for request in [independent, sought, opaque] {
            let (command, plan) =
                ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
                    HardwareAccelerationKind::None,
                    request,
                )
                .unwrap();
            assert!(!binding().validate(&command, plan.as_ref()));
        }
    }

    #[test]
    fn ordinary_fallback_can_bind_without_manufacturing_caption_authority() {
        let mut request = request(888);
        request.inputs.pop();
        let (command, plan) = ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
            HardwareAccelerationKind::None,
            request,
        )
        .unwrap();
        let mut binding = binding();
        binding.selected = None;
        assert!(plan.is_none());
        assert!(binding.validate(&command, None));
        binding.command = Some(command.clone());
        assert!(binding.matches(&command));
        let mut unrelated = command.clone();
        let source = unrelated
            .arguments
            .windows(2)
            .position(|pair| pair[0] == "-i")
            .unwrap();
        unrelated.arguments[source + 1] = "https://example.test/unrelated".into();
        assert!(!binding.validate(&unrelated, None));
        let mut duplicate = command;
        duplicate
            .arguments
            .splice(source..source, ["-i".into(), "pipe:0".into()]);
        assert!(!binding.validate(&duplicate, None));
    }

    #[tokio::test]
    async fn cancellation_and_expiry_precede_starting_any_producer() {
        let command = RemoteStreamCommand {
            tool_paths: Vec::new(),
            program: "absent-caption-producer".into(),
            args: Vec::new(),
            working_directory: std::path::PathBuf::from("."),
            environment: std::collections::BTreeMap::default(),
        };
        let extractor = FfprobeExtractor::new("absent-caption-probe");
        let (sender, receiver) = oneshot::channel();
        drop(sender);
        assert!(matches!(
            prepare_native_caption_producer(
                &command,
                &extractor,
                RemoteStreamSpawnContext::probe(),
                Instant::now() + Duration::from_secs(1),
                receiver
            )
            .await,
            Err(RemoteStreamError::PreparedCancelled)
        ));
        let (_sender, receiver) = oneshot::channel();
        assert!(matches!(
            prepare_native_caption_producer(
                &command,
                &extractor,
                RemoteStreamSpawnContext::probe(),
                Instant::now(),
                receiver
            )
            .await,
            Err(RemoteStreamError::PreparedTimeout)
        ));
    }
}
