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
        }),
        tracks: None,
        watermark: None,
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
