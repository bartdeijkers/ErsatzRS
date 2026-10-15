//! Genuine pre-decode ownership and same-byte page-header integration.

use super::*;
use crate::ffprobe::teletext_decode::PreparedLocalTeletextSource;

impl LaterSource {
    async fn prepare(
        &self,
        scope: &LocalProbeCleanupScope,
        work: &TeletextWorkGuard,
        page: u16,
        program: u16,
    ) -> PreparedLocalTeletextSource {
        let mut request = self.request.clone();
        request.page = TeletextPageNumber::new(page).unwrap();
        request.program_number = program;
        scope
            .scope(self.extractor.prepare_local_teletext_source(
                request,
                self.path.clone(),
                self.path.clone(),
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await
            .expect("native timing acquisition needs neither prefix page nor bitmap")
    }

    pub(super) async fn assert_prepared_source(
        &self,
        scope: &LocalProbeCleanupScope,
        work: &TeletextWorkGuard,
    ) {
        assert!(self
            .bytes
            .chunks_exact(PACKET_BYTES)
            .take(MAX_SOURCE_OBSERVATION_PACKETS)
            .all(|packet| packet_pid(packet) != SELECTED_PID));
        let prepared = self.prepare(scope, work, 888, 1).await;
        assert_eq!(prepared.request(), &self.request);
        let native = prepared.native_origin();
        assert_eq!(native.format_start().unwrap().units(), 1_418_667);
        assert_eq!(native.format_start().unwrap().scale(), 1_000_000);
        assert_eq!(native.streams().len(), 3);
        let decoded = scope
            .scope(self.extractor.decode_prepared_local_teletext_page(
                &prepared,
                self.span,
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await
            .expect("same prepared source decodes an actual later page");
        assert!(std::sync::Arc::ptr_eq(&prepared.guard, &decoded.guard));
        self.assert_exact_span(&decoded);
        assert!(decoded.events().count > 0);
        let headers = decoded.headers().expect("actual same-decoder-byte headers");
        assert_eq!(headers.elementary_pid(), SELECTED_PID);
        assert_eq!(
            u32::try_from(headers.packets()).unwrap(),
            decoded.window().decoder_packets
        );
        assert!(!headers.has_unobserved_prefix());
        for page in [200, 888] {
            assert!(headers.headers().iter().any(|item| item
                .header
                .display_page()
                .map(TeletextPageNumber::get)
                == Some(page)));
        }
        assert!(headers
            .headers()
            .iter()
            .all(|item| item.completion_packet < headers.packets()));
        // A program/page mismatch must fail before invoking this nonexistent
        // bitmap decoder, while native preparation itself remains available.
        let no_decoder = FfprobeExtractor::new("unavailable-synthetic-page-decoder");
        for (page, program, expected) in [
            (777, 1, SourceTeletextPageDecodeError::UnobservedPage),
            (
                888,
                2,
                SourceTeletextPageDecodeError::InvalidTransportWindow,
            ),
        ] {
            let candidate = self.prepare(scope, work, page, program).await;
            assert!(
                matches!(scope.scope(no_decoder.decode_prepared_local_teletext_page(
                &candidate, self.span, Instant::now() + Duration::from_secs(15), work.hold_work(),
            )).await, Err(error) if error == expected)
            );
        }
        assert!(matches!(
            self.extractor
                .decode_prepared_local_teletext_page(
                    &prepared,
                    self.span,
                    Instant::now() + Duration::from_secs(15),
                    work.hold_work(),
                )
                .await,
            Err(SourceTeletextPageDecodeError::NoCleanupScope)
        ));
        let modified = std::fs::metadata(&self.path).unwrap().modified().unwrap();
        let file = std::fs::File::options()
            .write(true)
            .open(&self.path)
            .unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(modified + Duration::from_secs(2)))
            .unwrap();
        assert!(matches!(
            scope
                .scope(no_decoder.decode_prepared_local_teletext_page(
                    &prepared,
                    self.span,
                    Instant::now() + Duration::from_secs(15),
                    work.hold_work(),
                ))
                .await,
            Err(SourceTeletextPageDecodeError::SourceChanged)
        ));
        file.set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        eprintln!(
            "pre-decode native origin: {:?}; same-byte headers={}, bitmaps={}, decoder_sha256={}",
            native,
            headers.headers().len(),
            decoded.events().count,
            hex_text(&decoded.window().decoder_sha256)
        );
    }
}
