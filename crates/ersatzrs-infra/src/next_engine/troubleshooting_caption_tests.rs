//! R292-D: zero-ID preview grants retain frozen media and one owning session.
use super::*;

async fn automatic_fixture() -> Fixture {
    let mut f = Fixture::new().await;
    f.channel.id = 0;
    f.channel.number = ".row292".into();
    f.channel.subtitle_mode = ersatzrs_domain::ChannelSubtitleMode::Any;
    f.channel.preferred_subtitle_language_code = Some("nld".into());
    f.media.item.id = 0;
    f.host
        .media
        .configure("127.0.0.1:18419".parse().unwrap())
        .unwrap();
    sqlx::query(r#"DELETE FROM "PlayoutItem""#)
        .execute(&f.pool)
        .await
        .unwrap();
    f
}

async fn publication(f: &Fixture) -> (uuid::Uuid, String, String) {
    let (instance, item) = f
        .host
        .prepare_troubleshooting_session(f.request(None), f.start())
        .await
        .unwrap();
    let track = item.tracks.unwrap().subtitle.unwrap();
    assert_eq!(track.stream_index, Some(0));
    let Some(protocol::Source::Http {
        uri, in_point_ms, ..
    }) = track.source
    else {
        panic!("automatic local preview must use its owned caption relay");
    };
    assert_eq!(in_point_ms, Some(1000));
    let uri = url::Url::parse(&uri).unwrap();
    let parts: Vec<_> = uri.path_segments().unwrap().collect();
    (instance, parts[2].into(), parts[3].into())
}

#[tokio::test]
async fn preview_caption_grants_use_frozen_media_without_sql_playout_and_never_adopt_new_ids() {
    for change in ["file", "path", "row", "replacement"] {
        let f = automatic_fixture().await;
        let (instance, encoded, signature) = publication(&f).await;
        assert!(f
            .host
            .read_caption(&encoded, &signature)
            .await
            .unwrap()
            .text
            .contains("Fixture"));
        match change {
            "file" => tokio::fs::remove_file(f.root.path().join("fixture.nl.srt"))
                .await
                .unwrap(),
            "path" => {
                sqlx::query(r#"UPDATE "Subtitle" SET "Path"=NULL WHERE "Id"=2"#)
                    .execute(&f.pool)
                    .await
                    .unwrap();
            }
            "row" => {
                sqlx::query(r#"DELETE FROM "Subtitle" WHERE "Id"=2"#)
                    .execute(&f.pool)
                    .await
                    .unwrap();
            }
            _ => {
                sqlx::query(r#"UPDATE "Subtitle" SET "Id"=2002 WHERE "Id"=2"#)
                    .execute(&f.pool)
                    .await
                    .unwrap();
            }
        }
        let empty = f.host.read_caption(&encoded, &signature).await.unwrap();
        assert!(!empty.text.contains("Fixture"));
        assert_eq!(empty.content_type, "application/x-subrip; charset=utf-8");
        f.host
            .finish_troubleshooting_session(instance)
            .await
            .unwrap();
        assert!(f.host.read_caption(&encoded, &signature).await.is_err());
    }
}

#[tokio::test]
async fn preview_caption_grants_revoke_media_and_owner_and_cannot_authorize_av_or_explicit_choices()
{
    let f = automatic_fixture().await;
    let (first, encoded, signature) = publication(&f).await;
    let (second, other_encoded, other_signature) = publication(&f).await;
    assert_ne!(first, second);
    let digest = format!("caption.{encoded}.{signature}");
    assert!(f
        .host
        .resolve_dynamic(
            0,
            0,
            &digest,
            Some(2),
            &f.channel.number,
            first,
            f.media.item.start,
            f.media.item.finish
        )
        .await
        .is_err());
    f.host.finish_troubleshooting_session(first).await.unwrap();
    assert!(f.host.read_caption(&encoded, &signature).await.is_err());
    assert!(f
        .host
        .read_caption(&other_encoded, &other_signature)
        .await
        .is_ok());
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"='replacement.mkv' WHERE "Id"=1"#)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(f
        .host
        .read_caption(&other_encoded, &other_signature)
        .await
        .is_err());
    f.host.finish_troubleshooting_session(second).await.unwrap();
    let f = automatic_fixture().await;
    let (instance, item) = f
        .host
        .prepare_troubleshooting_session(f.request(Some(2)), f.start())
        .await
        .unwrap();
    assert!(matches!(
        item.tracks.unwrap().subtitle.unwrap().source,
        Some(protocol::Source::Local { .. })
    ));
    f.host
        .finish_troubleshooting_session(instance)
        .await
        .unwrap();
}
