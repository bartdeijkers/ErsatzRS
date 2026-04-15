mod common;

use nastv_core::repositories::MetadataRepository;
use nastv_domain::{EpisodeMetadata, Metadata, MetadataKind, MovieMetadata};
use nastv_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn metadata_repository_round_trips_movie_and_episode_metadata() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_media_path(&pool, 1, 1, 1).await;
    common::seed_media_item(&pool, 10, 1).await;
    common::seed_media_item(&pool, 11, 1).await;
    pool.execute("INSERT INTO movies (id) VALUES (10)")
        .await
        .unwrap();
    pool.execute("INSERT INTO episodes (id, season_number, episode_number) VALUES (11, 1, 2)")
        .await
        .unwrap();

    repos
        .metadata
        .upsert_movie_metadata(&MovieMetadata {
            metadata: metadata(10, "Movie"),
            outline: Some("outline".to_owned()),
            plot: Some("plot".to_owned()),
            tagline: Some("tagline".to_owned()),
        })
        .await
        .unwrap();
    repos
        .metadata
        .upsert_episode_metadata(&EpisodeMetadata {
            metadata: metadata(11, "Episode"),
            season_number: 1,
            episode_number: 2,
            outline: Some("episode outline".to_owned()),
            plot: Some("episode plot".to_owned()),
        })
        .await
        .unwrap();

    let movie = repos.metadata.get_movie_metadata(10).await.unwrap();
    assert_eq!(movie.metadata.title, "Movie");
    assert!(movie.metadata.artwork_ids.is_empty());

    let episode = repos.metadata.get_episode_metadata(11).await.unwrap();
    assert_eq!(episode.season_number, 1);
    assert_eq!(episode.episode_number, 2);
}

fn metadata(id: i64, title: &str) -> Metadata {
    Metadata {
        id,
        metadata_kind: MetadataKind::Fallback,
        title: title.to_owned(),
        date_added: common::fixed_time(),
        date_updated: common::fixed_time(),
        ..Metadata::default()
    }
}
