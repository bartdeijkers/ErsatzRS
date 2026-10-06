//! The existing audio callback contract for programmes without database IDs.

use super::{
    execute, read_script_bytes, record_failure, AudioScriptPreferences, Request, SCRIPT_BYTES,
};
use crate::playback::transient::TransientPlaybackTracks;
use ersatzrs_core::language_codes;
use ersatzrs_domain::{Channel, StreamingMode};
use serde_json::json;

/// Programme metadata passed to the same movie/episode callbacks as stored media.
pub enum AudioMetadata<'a> {
    Movie {
        title: Option<&'a str>,
        guids: &'a [String],
    },
    Episode {
        show_title: Option<&'a str>,
        show_guids: &'a [String],
        season: i32,
        episode: i32,
        guids: &'a [String],
    },
}

/// Attempt the configured callback after pins and custom YAML selection.
/// Failures use the existing bounded VM and ordinary-selection fallback.
pub async fn select(
    channel: &Channel,
    metadata: AudioMetadata<'_>,
    preferences: AudioScriptPreferences<'_>,
    tracks: &TransientPlaybackTracks,
) -> Option<usize> {
    if preferences.streaming_mode == StreamingMode::HttpLiveStreamingDirect
        && preferences
            .language
            .is_none_or(|value| value.trim().is_empty())
        && preferences
            .title
            .is_none_or(|value| value.trim().is_empty())
    {
        return None;
    }
    let folder = crate::file_system_layout::scripts_folder().join("audio-stream-selector");
    match request(&folder, channel, metadata, preferences.language, tracks).await {
        Ok(Some((script, callback, arguments))) => {
            execute(
                script,
                callback,
                arguments,
                tracks
                    .audio
                    .iter()
                    .filter_map(|track| usize::try_from(track.stream_index).ok())
                    .collect(),
            )
            .await
        }
        Ok(None) => None,
        Err(_) => {
            record_failure();
            tracing::warn!("audio selector could not be loaded; using built-in selection");
            None
        }
    }
}

async fn request(
    folder: &std::path::Path,
    channel: &Channel,
    metadata: AudioMetadata<'_>,
    language: Option<&str>,
    tracks: &TransientPlaybackTracks,
) -> Result<Option<Request>, Box<dyn std::error::Error + Send + Sync>> {
    let (kind, callback) = match metadata {
        AudioMetadata::Movie { .. } => ("movie", "selectMovieAudioStreamIndex"),
        AudioMetadata::Episode { .. } => ("episode", "selectEpisodeAudioStreamIndex"),
    };
    let Some(bytes) = read_script_bytes(&folder.join(format!("{kind}.js"))).await? else {
        return Ok(None);
    };
    let script = String::from_utf8(bytes)?;
    if script.len() > SCRIPT_BYTES {
        return Err("script size limit exceeded".into());
    }
    let mut arguments = vec![json!(channel.number), json!(channel.name)];
    match metadata {
        AudioMetadata::Movie { title, guids } => {
            if guids.len() > 256 {
                return Err("guid size limit exceeded".into());
            }
            arguments.extend([json!(title), json!(script_guids(guids))]);
        }
        AudioMetadata::Episode {
            show_title,
            show_guids,
            season,
            episode,
            guids,
        } => {
            if show_guids.len() > 256 || guids.len() > 256 {
                return Err("guid size limit exceeded".into());
            }
            arguments.extend([
                json!(show_title),
                json!(script_guids(show_guids)),
                json!(season),
                json!(episode),
                json!(script_guids(guids)),
            ]);
        }
    }
    arguments.push(json!(language_codes::all_language_codes(language)));
    if tracks.audio.len() > 256 {
        return Err("audio metadata size limit exceeded".into());
    }
    arguments.push(json!(tracks
        .audio
        .iter()
        .map(|track| json!({
            "index": track.stream_index,
            "channels": track.channels.unwrap_or_default(),
            "codec": track.codec,
            "isDefault": track.default,
            "isForced": track.forced,
            "language": track.language,
            "title": track.title,
        }))
        .collect::<Vec<_>>()));
    if serde_json::to_vec(&arguments)?.len() > SCRIPT_BYTES {
        return Err("selector metadata size limit exceeded".into());
    }
    Ok(Some((script, callback, arguments)))
}

fn script_guids(values: &[String]) -> Vec<String> {
    values.iter().map(|guid| guid.replace("://", "_")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{MediaStream, MediaStreamKind};

    fn tracks() -> TransientPlaybackTracks {
        TransientPlaybackTracks::from_streams(&[MediaStream {
            index: 7,
            kind: MediaStreamKind::Audio,
            codec: Some("truehd".into()),
            channels: Some(6),
            language: Some("nld".into()),
            title: Some("Dutch".into()),
            default: false,
            forced: true,
            ..MediaStream::default()
        }])
    }

    #[tokio::test]
    async fn transient_audio_callbacks_receive_metadata_and_original_tracks_without_sql() {
        let root = tempfile::tempdir().unwrap();
        let channel = Channel {
            number: "7".into(),
            name: "Fixture".into(),
            ..Default::default()
        };
        let tracks = tracks();
        for (file, metadata, script) in [
            ("movie.js", AudioMetadata::Movie { title: Some("Movie"), guids: &["tmdb://42".into()] },
             "function selectMovieAudioStreamIndex(number,name,title,guids,languages,audio) { if(number !== '7' || name !== 'Fixture' || title !== 'Movie' || guids[0] !== 'tmdb_42' || !languages.includes('nld') || audio[0].codec !== 'truehd' || audio[0].channels !== 6 || audio[0].isDefault || !audio[0].isForced || audio[0].title !== 'Dutch') throw Error('shape'); return audio[0].index; }"),
            ("episode.js", AudioMetadata::Episode { show_title: Some("Show"), show_guids: &["tvdb://10".into()], season: 2, episode: 3, guids: &["tvdb://11".into()] },
             "function selectEpisodeAudioStreamIndex(number,name,show,showGuids,season,episode,guids,languages,audio) { if(show !== 'Show' || showGuids[0] !== 'tvdb_10' || season !== 2 || episode !== 3 || guids[0] !== 'tvdb_11' || !languages.includes('nl') || audio[0].language !== 'nld') throw Error('shape'); return audio[0].index; }")
        ] {
            tokio::fs::write(root.path().join(file), script).await.unwrap();
            let (script, callback, arguments) = request(root.path(), &channel, metadata, Some("nl"), &tracks).await.unwrap().unwrap();
            assert_eq!(execute(script, callback, arguments, vec![7]).await, Some(7));
        }
    }

    #[tokio::test]
    async fn transient_audio_request_preserves_absence_and_size_limits() {
        let root = tempfile::tempdir().unwrap();
        let channel = Channel::default();
        let metadata = || AudioMetadata::Movie {
            title: None,
            guids: &[],
        };
        let tracks = tracks();
        assert!(request(root.path(), &channel, metadata(), None, &tracks)
            .await
            .unwrap()
            .is_none());
        tokio::fs::write(root.path().join("movie.js"), "x".repeat(SCRIPT_BYTES + 1))
            .await
            .unwrap();
        assert!(request(root.path(), &channel, metadata(), None, &tracks)
            .await
            .is_err());
        tokio::fs::write(
            root.path().join("movie.js"),
            "function selectMovieAudioStreamIndex() {}",
        )
        .await
        .unwrap();
        let oversized = TransientPlaybackTracks {
            audio: vec![tracks.audio[0].clone(); 257],
            subtitles: vec![],
        };
        assert!(request(root.path(), &channel, metadata(), None, &oversized)
            .await
            .is_err());
    }
}
