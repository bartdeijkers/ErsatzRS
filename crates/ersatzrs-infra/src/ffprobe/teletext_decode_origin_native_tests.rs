//! Native original-source start timing, compared with the held raw AV read.

use super::*;

impl LaterSource {
    pub(super) async fn assert_native_av_origin(
        &self,
        scope: &LocalProbeCleanupScope,
        work: &TeletextWorkGuard,
        decoded: &DecodedLocalTeletextPage,
    ) {
        assert!(matches!(
            self.extractor
                .observe_local_teletext_av_origin(
                    decoded,
                    Instant::now() + Duration::from_secs(10),
                    work.hold_work()
                )
                .await,
            Err(SourceTeletextPageDecodeError::NoCleanupScope)
        ));
        let observed = scope
            .scope(self.extractor.observe_local_teletext_av_origin(
                decoded,
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await
            .unwrap();
        let format = observed.native().format_start().unwrap();
        assert_eq!((format.units(), format.scale()), (1_418_667, 1_000_000));
        assert_eq!(format.quantum(), (1, 1_000_000));
        assert_eq!(observed.streams().len(), 3);
        let video = observed
            .streams()
            .iter()
            .find(|stream| stream.native().identity().pid == 256)
            .unwrap();
        assert_eq!(video.native().identity().stream_index, 0);
        assert_eq!(video.native().start_pts(), Some(129_600));
        let base = video.native().time_base().unwrap();
        assert_eq!((base.numerator(), base.denominator()), (1, 90_000));
        assert_eq!(video.raw().first_pts.unwrap().raw_ticks_90khz, 129_600);
        assert_eq!(video.raw().first_dts.unwrap().raw_ticks_90khz, 126_000);
        assert_eq!(video.raw().headers_seen, 1_500);
        assert_eq!(video.matched_native_start(), video.raw().first_pts);
        assert_eq!(video.native_start_near_format_origin(), Some(false));
        for (pid, index, first) in [(257, 1, 427), (259, 3, 443)] {
            let audio = observed
                .streams()
                .iter()
                .find(|stream| stream.native().identity().pid == pid)
                .unwrap();
            assert_eq!(audio.native().identity().stream_index, index);
            assert_eq!(audio.native().start_pts(), Some(127_680));
            assert_eq!(audio.raw().headers_seen, 176);
            let sample = audio.raw().first_pts.unwrap();
            assert_eq!(sample.source_pusi_packet, first);
            assert_eq!(sample.raw_ticks_90khz, 127_680);
            assert_eq!(audio.matched_native_start(), Some(sample));
            assert_eq!(audio.raw().first_dts, None);
            assert_eq!(audio.native_start_near_format_origin(), Some(true));
        }
        let clock = observed.clock().clock();
        assert_eq!(clock.packets_seen, 79_769);
        assert_ne!(
            i128::from(clock.first.unwrap().raw_ticks),
            format.units() * 27
        );
        let expected: [u8; 32] = Sha256::digest(&self.bytes).into();
        assert_eq!(observed.clock().source_sha256(), expected);
        for stream in observed.streams() {
            assert_eq!(stream.raw().packets_seen, clock.packets_seen);
        }
        eprintln!("later-source native AV origin: {observed:?}");

        // Changed metadata must fail before launching the optional native probe.
        let modified = std::fs::metadata(&self.path).unwrap().modified().unwrap();
        let file = std::fs::File::options()
            .write(true)
            .open(&self.path)
            .unwrap();
        file.set_times(
            std::fs::FileTimes::new()
                .set_modified(modified.checked_add(Duration::from_secs(2)).unwrap()),
        )
        .unwrap();
        assert!(matches!(
            scope
                .scope(self.extractor.observe_local_teletext_av_origin(
                    decoded,
                    Instant::now() + Duration::from_secs(10),
                    work.hold_work()
                ))
                .await,
            Err(SourceTeletextPageDecodeError::SourceChanged)
        ));
        file.set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
    }
}
