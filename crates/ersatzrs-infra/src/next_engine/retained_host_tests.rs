use super::*;
use ersatzrs_ffmpeg::{SourceAudioCodec, SourceAudioStream};

#[tokio::test]
async fn retained_preflight_observes_stop_and_deadline_while_database_is_busy() {
    for cancelled in [true, false] {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let held = pool.acquire().await.unwrap();
        let (cancel, cancellation) = oneshot::channel();
        let (subtitle, _) = oneshot::channel();
        let preparation = RetainedPreparation {
            pool: pool.clone(),
            media_id: 1,
            ffprobe: "unused-ffprobe".into(),
            number: "1".into(),
            selected_subtitle: None,
            programme_ms: 0,
            max_height: None,
            preparation_deadline: Instant::now()
                + if cancelled {
                    std::time::Duration::from_secs(30)
                } else {
                    std::time::Duration::from_millis(20)
                },
        };
        let operation = preparation.run(cancellation, subtitle);
        tokio::pin!(operation);
        // Poll the actual SQL acquisition while its only connection is held.
        assert!(futures_util::poll!(&mut operation).is_pending());
        if cancelled {
            cancel.send(()).unwrap();
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), operation)
            .await
            .expect("preflight must not wait for the pool acquisition timeout");
        if cancelled {
            assert!(matches!(result, Err(RemoteStreamError::PreparedCancelled)));
        } else {
            assert!(matches!(result, Err(RemoteStreamError::PreparedTimeout)));
        }
        drop(held);
        assert!(pool.acquire().await.is_ok());
    }
}

fn fixture() -> (
    FfprobeMediaVersion,
    Vec<SourceAudioStream>,
    protocol::PlayoutItem,
) {
    let mut probe = super::super::probe_hint::tests::metadata();
    probe.streams[1].language = Some("nld".into());
    let mut english = probe.streams[1].clone();
    english.index = 24;
    english.language = Some("eng".into());
    probe.streams.push(english);
    let previous = [(1, "eng"), (2, "nld")]
        .into_iter()
        .map(|(index, language)| {
            let mut audio = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(index);
            audio.language = Some(language.into());
            audio
        })
        .collect();
    let start = Utc::now();
    let item = protocol::PlayoutItem {
        id: "1".into(),
        start,
        finish: start + chrono::Duration::seconds(50),
        source: Some(protocol::Source::Local {
            path: "old-source.mkv".into(),
            in_point_ms: Some(12_000),
            out_point_ms: Some(62_000),

            probe_hint: None,
        }),
        tracks: None,
        watermark: None,
        graphics: Vec::new(),
    };
    (probe, previous, item)
}

fn source(probe: &FfprobeMediaVersion) -> protocol::Source {
    let relay = super::super::media::MediaRelay::new();
    relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
    relay
        .prepared_source(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            super::super::probe_hint::retained(probe, 40_000).unwrap(),
            false,
        )
        .unwrap()
}

#[test]
fn audio_only_producer_preserves_generated_picture_and_exact_audio_after_probe() {
    use ersatzrs_domain::{MediaStreamKind, Resolution};
    for song in [false, true] {
        for selected in [None, Some(2)] {
            let (mut probe, previous, mut item) = fixture();
            probe.streams[0].attached_pic = true;
            let hint = super::super::probe_hint::generated_video(
                &Resolution {
                    width: 320,
                    height: 180,
                    ..Default::default()
                },
                50_000,
                if song { "png" } else { "rawvideo" },
                song.then_some("png_pipe"),
            )
            .unwrap();
            let picture = if song {
                serde_json::from_value(serde_json::json!({
                    "source_type": "http", "uri": "http://127.0.0.1/ffmpeg/next-song-background?binding=synthetic",
                    "probe_hint": hint,
                })).unwrap()
            } else {
                protocol::Source::Lavfi {
                    params: "color=c=black:s=320x180".into(),
                    probe_hint: Some(hint),
                }
            };
            let expected = serde_json::to_value(&picture).unwrap();
            item.tracks = Some(protocol::Tracks {
                video: Some(protocol::Track {
                    source: Some(picture),
                    stream_index: Some(0),
                }),
                audio: None,
                subtitle: None,
            });
            let now = item.start + chrono::Duration::seconds(10);
            let prepared = protocol::Source::Local {
                path: "owned-producer.nut".into(),
                in_point_ms: None,
                out_point_ms: None,
                probe_hint: None,
            };
            let resolved = install_fresh_tracks(
                item.clone(),
                &probe,
                &previous,
                selected,
                prepared.clone(),
                None,
                now,
            )
            .unwrap();
            let tracks = resolved.tracks.unwrap();
            let video = tracks.video.unwrap();
            assert_eq!(video.stream_index, Some(0));
            assert_eq!(
                serde_json::to_value(video.source.unwrap()).unwrap(),
                expected
            );
            let audio = tracks.audio.unwrap();
            if selected.is_some() {
                assert_eq!(audio.stream_index, Some(23));
                assert!(
                    matches!(audio.source, Some(protocol::Source::Local { path, .. }) if path == "owned-producer.nut")
                );
            } else {
                assert!(matches!(audio.source, Some(protocol::Source::Lavfi { .. })));
            }
            // Retained normalization can add raw black video to an audio-only
            // song; that fresh index must not erase its published picture.
            probe.streams[0].attached_pic = false;
            let normalized = install_fresh_tracks(
                item.clone(),
                &probe,
                &previous,
                selected,
                prepared.clone(),
                None,
                now,
            )
            .unwrap();
            let normalized_video = normalized.tracks.unwrap().video.unwrap();
            assert_eq!(normalized_video.stream_index, Some(0));
            assert_eq!(
                serde_json::to_value(normalized_video.source.unwrap()).unwrap(),
                expected
            );
            // A producer losing its original audio must fail rather than publish
            // an apparently healthy, silently frozen programme.
            probe
                .streams
                .retain(|stream| stream.kind != MediaStreamKind::Audio);
            assert!(
                install_fresh_tracks(item, &probe, &previous, selected, prepared, None, now)
                    .is_err()
            );
        }
    }
}

#[test]
fn fresh_retained_indexes_replace_old_indexes_without_second_seek() {
    let (probe, previous, item) = fixture();
    let now = item.start + chrono::Duration::seconds(10);
    let finish = item.finish;
    let resolved = install_fresh_tracks(
        item,
        &probe,
        &previous,
        Some(2),
        source(&probe),
        Some("verified-rebased.srt".into()),
        now,
    )
    .unwrap();
    assert_eq!(resolved.start, now);
    assert_eq!(resolved.finish, finish);
    assert!(resolved.source.is_none());
    let tracks = resolved.tracks.unwrap();
    assert_eq!(tracks.video.as_ref().unwrap().stream_index, Some(17));
    assert_eq!(tracks.audio.as_ref().unwrap().stream_index, Some(23));
    for track in [tracks.video.unwrap(), tracks.audio.unwrap()] {
        assert!(matches!(
            track.source,
            Some(protocol::Source::Http {
                in_point_ms: None,
                out_point_ms: None,
                ..
            })
        ));
    }
    assert!(matches!(
        tracks.subtitle.unwrap().source,
        Some(protocol::Source::Local {
            in_point_ms: None,
            out_point_ms: None,
            ..
        })
    ));
}

#[test]
fn explicit_silence_survives_and_ambiguous_audio_is_refused() {
    let (mut probe, previous, item) = fixture();
    let now = item.start + chrono::Duration::seconds(10);
    let resolved = install_fresh_tracks(
        item.clone(),
        &probe,
        &previous,
        None,
        source(&probe),
        None,
        now,
    )
    .unwrap();
    assert!(matches!(
        resolved.tracks.unwrap().audio.unwrap().source,
        Some(protocol::Source::Lavfi { .. })
    ));
    probe.streams[2].language = Some("nld".into());
    assert!(
        install_fresh_tracks(item, &probe, &previous, Some(2), source(&probe), None, now).is_err()
    );
}
