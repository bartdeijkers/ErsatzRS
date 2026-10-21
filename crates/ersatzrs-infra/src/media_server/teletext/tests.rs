//! Provider declarations and native layouts must agree before acquisition can bind.
use super::{caption_physical_original, declaration_matches, is_teletext, native_tracks_match};
use ersatzrs_core::repositories::{ProviderMediaSourceDeclaration, RemoteMediaSourceKind};
use ersatzrs_domain::{
    MediaSourceKind, MediaStream, MediaStreamKind, SourceTeletextBindingIdentity,
    SourceTeletextRevision,
};

fn track(index: i32, kind: MediaStreamKind, codec: &str) -> MediaStream {
    MediaStream {
        index,
        kind,
        codec: Some(codec.to_owned()),
        ..MediaStream::default()
    }
}

fn tracks() -> Vec<MediaStream> {
    vec![
        track(0, MediaStreamKind::Video, "h264"),
        track(1, MediaStreamKind::Audio, "aac"),
        track(2, MediaStreamKind::Subtitle, "dvb_teletext"),
    ]
}

fn declaration() -> ProviderMediaSourceDeclaration {
    ProviderMediaSourceDeclaration::new(
        "selected-source".to_owned(),
        "/provider/synthetic.ts".to_owned(),
        [Some(0), Some(1), Some(2)],
    )
    .unwrap()
}

#[test]
fn exact_native_layout_preserves_provider_sidecars_without_using_their_indexes() {
    let mut provider = tracks();
    let native = provider.clone();
    provider.push(track(0, MediaStreamKind::ExternalSubtitle, "subrip"));
    provider.reverse();
    let declaration = declaration();
    assert!(declaration_matches(&declaration, &provider));
    assert!(native_tracks_match(&provider, &native));
    assert_eq!(
        provider.iter().filter(|stream| is_teletext(stream)).count(),
        1
    );
    let external = track(2, MediaStreamKind::ExternalSubtitle, "dvb_teletext");
    assert!(!is_teletext(&external));
    assert!(!declaration_matches(&declaration, &[external]));
}

#[test]
fn incomplete_shifted_and_ambiguous_declarations_cannot_bind() {
    let provider = tracks();
    for indexes in [
        vec![],
        vec![0, 1],
        vec![0, 1, 3],
        vec![0, 1, 2, 3],
        vec![0, 1, 2, 2],
    ] {
        let mut declaration = declaration();
        declaration.stream_indexes = indexes;
        assert!(!declaration_matches(&declaration, &provider));
    }
    let mut duplicate = provider.clone();
    duplicate.push(provider[2].clone());
    let mut declaration = declaration();
    declaration.stream_indexes.push(2);
    assert!(!declaration_matches(&declaration, &duplicate));
    let mut negative = provider;
    negative[0].index = -1;
    declaration.stream_indexes = vec![-1, 1, 2];
    assert!(!declaration_matches(&declaration, &negative));
}

#[test]
fn native_correspondence_requires_every_index_kind_and_codec() {
    let provider = tracks();
    let mut shifted = provider.clone();
    shifted[2].index = 3;
    let mut missing = provider.clone();
    missing.pop();
    let mut extra = provider.clone();
    extra.push(track(3, MediaStreamKind::Audio, "aac"));
    let mut duplicate = provider.clone();
    duplicate[2].index = 1;
    let mut wrong_kind = provider.clone();
    wrong_kind[2].kind = MediaStreamKind::Audio;
    let mut wrong_codec = provider.clone();
    wrong_codec[2].codec = Some("subrip".to_owned());
    for native in [shifted, missing, extra, duplicate, wrong_kind, wrong_codec] {
        assert!(!native_tracks_match(&provider, &native));
    }
    assert!(declaration_matches(&declaration(), &provider));
    assert!(native_tracks_match(&provider, &provider));
}

/// Mapping intent alone grants neither a readable file nor caption readiness.
#[tokio::test]
async fn caption_locator_requires_the_current_owning_provider_mapping() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (11),(22)"#)
        .execute(&pool)
        .await
        .unwrap();
    for kind in RemoteMediaSourceKind::ALL {
        let provider = kind.provider_name();
        let domain = match kind {
            RemoteMediaSourceKind::Plex => MediaSourceKind::Plex,
            RemoteMediaSourceKind::Jellyfin => MediaSourceKind::Jellyfin,
            RemoteMediaSourceKind::Emby => MediaSourceKind::Emby,
        };
        // All identifiers come from the closed provider enum.
        sqlx::query(crate::db::assert_sql_safe(format!(
            r#"INSERT INTO "{provider}MediaSource" ("Id") VALUES (11),(22)"#
        )))
        .execute(&pool)
        .await
        .unwrap();
        let table = format!("{provider}PathReplacement");
        let insert = format!(
            r#"INSERT INTO "{table}" ("{provider}MediaSourceId","{provider}Path","LocalPath") VALUES (?,?,?)"#
        );
        let source = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        for (id, folder) in [(11, &source), (22, &elsewhere)] {
            sqlx::query(crate::db::assert_sql_safe(&insert))
                .bind(id)
                .bind("/provider")
                .bind(folder.path().to_str().unwrap())
                .execute(&pool)
                .await
                .unwrap();
        }
        let mut binding = SourceTeletextBindingIdentity::new(
            11,
            domain,
            1,
            2,
            3,
            SourceTeletextRevision::new("synthetic-unopened-revision".into()).unwrap(),
        )
        .unwrap();
        let original = "/provider/movie.ts";
        let physical = source.path().join("movie.ts");
        let resolved = physical.to_str().unwrap();
        assert_eq!(
            caption_physical_original(&pool, &binding, original, resolved)
                .await
                .unwrap(),
            physical
        );
        binding.media_source_id = 22;
        assert!(
            caption_physical_original(&pool, &binding, original, resolved)
                .await
                .is_err()
        );
        binding.media_source_id = 11;
        sqlx::query(crate::db::assert_sql_safe(format!(
            r#"UPDATE "{table}" SET "LocalPath"=? WHERE "{provider}MediaSourceId"=11"#
        )))
        .bind(elsewhere.path().to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            caption_physical_original(&pool, &binding, original, resolved)
                .await
                .is_err()
        );
        sqlx::query(crate::db::assert_sql_safe(format!(
            r#"DELETE FROM "{table}" WHERE "{provider}MediaSourceId"=11"#
        )))
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            caption_physical_original(&pool, &binding, original, resolved)
                .await
                .is_err()
        );
        binding.provider = MediaSourceKind::Local;
        assert_eq!(
            caption_physical_original(&pool, &binding, original, resolved)
                .await
                .unwrap(),
            std::path::PathBuf::from(original),
            "Local must retain its distinct original-path guard"
        );
    }
}
