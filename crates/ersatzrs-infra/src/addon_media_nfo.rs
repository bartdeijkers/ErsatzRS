//! Managed, disk-authoritative Kodi-style NFO metadata for add-on media lists.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use ersatzrs_core::repositories::{
    AddonMediaListArtwork, AddonMediaListArtworkRole, AddonMediaListNfoForm,
    AddonMediaListNfoMetadata, AddonMediaListNfoOwnership, AddonMediaListNfoPerson,
};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

const STATE_FILE: &str = ".ersatzrs-nfo-state.json";

#[derive(Debug, thiserror::Error)]
pub enum AddonMediaNfoError {
    #[error("unable to read managed NFO state: {0}")]
    StateRead(String),
    #[error("unable to write managed NFO state: {0}")]
    StateWrite(String),
    #[error("unable to read managed NFO: {0}")]
    Read(String),
    #[error("unable to write managed NFO: {0}")]
    Write(String),
    #[error("managed NFO is invalid: {0}")]
    Invalid(String),
    #[error("managed NFO changed since the form was loaded")]
    Conflict,
    #[error("managed NFO path is not available until the list has been scanned")]
    PathUnavailable,
}

pub type Result<T> = std::result::Result<T, AddonMediaNfoError>;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct NfoState {
    version: u8,
    #[serde(default)]
    files: BTreeMap<String, NfoFileState>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct NfoFileState {
    ownership: AddonMediaListNfoOwnership,
    sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawNfo {
    name: Option<String>,
    title: Option<String>,
    originaltitle: Option<String>,
    sorttitle: Option<String>,
    outline: Option<String>,
    plot: Option<String>,
    tagline: Option<String>,
    showtitle: Option<String>,
    season: Option<String>,
    episode: Option<String>,
    year: Option<String>,
    aired: Option<String>,
    premiered: Option<String>,
    #[serde(rename = "ersatzrs:release_datetime", alias = "release_datetime")]
    release_datetime: Option<String>,
    #[serde(rename = "mpaa")]
    content_ratings: Vec<String>,
    #[serde(rename = "genre")]
    genres: Vec<String>,
    #[serde(rename = "tag")]
    tags: Vec<String>,
    #[serde(rename = "studio")]
    studios: Vec<String>,
    #[serde(rename = "label")]
    music_labels: Vec<String>,
    #[serde(rename = "language")]
    languages: Vec<String>,
    #[serde(rename = "actor")]
    people: Vec<RawPerson>,
    #[serde(rename = "credits")]
    writers: Vec<String>,
    #[serde(rename = "director")]
    directors: Vec<String>,
    #[serde(rename = "artist")]
    artists: Vec<String>,
    #[serde(rename = "ersatzrs:producer", alias = "producer")]
    producers: Vec<String>,
    #[serde(
        rename = "ersatzrs:original_broadcaster",
        alias = "original_broadcaster"
    )]
    original_broadcasters: Vec<String>,
    #[serde(rename = "ersatzrs:broadcaster", alias = "broadcaster")]
    broadcasters: Vec<String>,
    #[serde(rename = "ersatzrs:collection", alias = "collection")]
    collection: Option<String>,
    #[serde(rename = "thumb")]
    artwork: Vec<RawArtwork>,
    fanart: Option<RawFanart>,
    #[serde(rename = "uniqueid")]
    guids: Vec<RawGuid>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawPerson {
    name: Option<String>,
    role: Option<String>,
    order: Option<i32>,
    thumb: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawGuid {
    #[serde(rename = "@type")]
    source: Option<String>,
    #[serde(rename = "$text")]
    value: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawArtwork {
    #[serde(rename = "@aspect")]
    aspect: Option<String>,
    #[serde(rename = "@width")]
    width: Option<u32>,
    #[serde(rename = "@height")]
    height: Option<u32>,
    #[serde(rename = "@language")]
    language: Option<String>,
    #[serde(rename = "$text")]
    url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawFanart {
    #[serde(rename = "thumb")]
    thumbs: Vec<RawArtwork>,
}

/// Read and validate one managed NFO document.
///
/// # Errors
///
/// Returns an error when the file, ownership state, or XML cannot be read or decoded.
pub fn read_form(path: Option<&Path>) -> Result<AddonMediaListNfoForm> {
    let Some(path) = path else {
        return Ok(AddonMediaListNfoForm::default());
    };
    if !path.is_file() {
        return Ok(AddonMediaListNfoForm {
            etag: hash(&[]),
            path_available: true,
            conflicts: BTreeMap::new(),
            ..AddonMediaListNfoForm::default()
        });
    }
    let bytes = fs::read(path).map_err(|error| AddonMediaNfoError::Read(error.to_string()))?;
    let metadata = parse(&bytes)?;
    let state = load_state(path.parent().unwrap_or_else(|| Path::new(".")))?;
    let key = file_key(path)?;
    let record = state.files.get(&key);
    Ok(AddonMediaListNfoForm {
        metadata,
        etag: hash(&bytes),
        ownership: record.map_or(AddonMediaListNfoOwnership::Generated, |value| {
            value.ownership
        }),
        error: record.and_then(|value| value.error.clone()),
        path_available: true,
        conflicts: BTreeMap::new(),
    })
}

/// Save a user-owned NFO through optimistic concurrency and atomic publication.
///
/// # Errors
///
/// Returns an error for an `ETag` conflict, invalid metadata, or failed publication.
pub fn save_user(
    path: &Path,
    root: &str,
    metadata: &AddonMediaListNfoMetadata,
    expected_etag: &str,
) -> Result<()> {
    let current = fs::read(path).unwrap_or_default();
    if hash(&current) != expected_etag {
        return Err(AddonMediaNfoError::Conflict);
    }
    let bytes = serialize(root, metadata)?;
    parse(&bytes)?;
    publish_with_state(path, &bytes, AddonMediaListNfoOwnership::User, None)
}

/// Reset one document to generated ownership. The caller's next provider scan
/// supplies the generated bytes; removing the file prevents stale user data
/// from being imported in the meantime.
///
/// # Errors
///
/// Returns an error when the NFO or its ownership record cannot be removed.
pub fn reset(path: &Path) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut state = load_state(parent)?;
    state.files.remove(&file_key(path)?);
    if path.exists() {
        fs::remove_file(path).map_err(|error| AddonMediaNfoError::Write(error.to_string()))?;
    }
    save_state(parent, &state)
}

/// Preserve a valid user-edited NFO, otherwise publish provider-generated
/// bytes. The returned bytes are the descriptive metadata that the scanner
/// must import for this generation.
///
/// # Errors
///
/// Returns an error for invalid generated or user XML and failed state publication.
pub fn reconcile_generated(path: &Path, generated: &[u8]) -> Result<Vec<u8>> {
    parse(generated)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut state = load_state(parent)?;
    let key = file_key(path)?;
    let current = fs::read(path).ok();
    let record = state.files.get(&key).cloned();

    if let Some(current) = current {
        let current_hash = hash(&current);
        // An empty, untracked file is a common attempt to satisfy software
        // that reports a missing NFO. It contains no user metadata to preserve,
        // so treat it as an absent generated file instead of adopting it as an
        // invalid user document. Once a file has ownership state, the normal
        // user/invalid preservation rules still apply.
        if current.is_empty() && record.is_none() {
            publish_with_state(path, generated, AddonMediaListNfoOwnership::Generated, None)?;
            return Ok(generated.to_vec());
        }
        let changed_user_file = record.as_ref().is_some_and(|record| {
            record.ownership != AddonMediaListNfoOwnership::Generated
                || record.sha256 != current_hash
        }) || record.is_none() && current != generated;
        if changed_user_file {
            match parse(&current) {
                Ok(_) => {
                    state.files.insert(
                        key,
                        NfoFileState {
                            ownership: AddonMediaListNfoOwnership::User,
                            sha256: current_hash,
                            error: None,
                        },
                    );
                    save_state(parent, &state)?;
                    return Ok(current);
                }
                Err(error) => {
                    state.files.insert(
                        key,
                        NfoFileState {
                            ownership: AddonMediaListNfoOwnership::Invalid,
                            sha256: current_hash,
                            error: Some(error.to_string()),
                        },
                    );
                    save_state(parent, &state)?;
                    return Err(error);
                }
            }
        }
    } else if record.is_some_and(|record| {
        matches!(
            record.ownership,
            AddonMediaListNfoOwnership::User | AddonMediaListNfoOwnership::Invalid
        )
    }) {
        return Err(AddonMediaNfoError::Invalid(
            "a user-owned NFO file is missing".to_owned(),
        ));
    }

    publish_with_state(path, generated, AddonMediaListNfoOwnership::Generated, None)?;
    Ok(generated.to_vec())
}

/// Register a current managed file during a cache-hit scan. A hash difference
/// from the last host generation is a user edit and is validated before it is
/// allowed to become authoritative.
///
/// # Errors
///
/// Returns an error when the file is missing, invalid, or ownership cannot be persisted.
pub fn adopt_existing(path: &Path, generated_hash: Option<&str>) -> Result<()> {
    let bytes = fs::read(path).map_err(|error| AddonMediaNfoError::Read(error.to_string()))?;
    let current_hash = hash(&bytes);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut state = load_state(parent)?;
    let key = file_key(path)?;
    let prior = state.files.get(&key).cloned();
    let unchanged_generated = prior.as_ref().is_some_and(|record| {
        record.ownership == AddonMediaListNfoOwnership::Generated && record.sha256 == current_hash
    }) || prior.is_none()
        && generated_hash == Some(current_hash.as_str());
    if unchanged_generated {
        state.files.insert(
            key,
            NfoFileState {
                ownership: AddonMediaListNfoOwnership::Generated,
                sha256: current_hash,
                error: None,
            },
        );
        return save_state(parent, &state);
    }
    match parse(&bytes) {
        Ok(_) => {
            state.files.insert(
                key,
                NfoFileState {
                    ownership: AddonMediaListNfoOwnership::User,
                    sha256: current_hash,
                    error: None,
                },
            );
            save_state(parent, &state)
        }
        Err(error) => {
            state.files.insert(
                key,
                NfoFileState {
                    ownership: AddonMediaListNfoOwnership::Invalid,
                    sha256: current_hash,
                    error: Some(error.to_string()),
                },
            );
            save_state(parent, &state)?;
            Err(error)
        }
    }
}

/// Encode a deterministic Kodi-style document and validate its round trip.
///
/// # Errors
///
/// Returns an error for an unsupported root or XML encoding failure.
#[allow(clippy::too_many_lines)]
pub fn serialize(root: &str, metadata: &AddonMediaListNfoMetadata) -> Result<Vec<u8>> {
    if !matches!(
        root,
        "tvshow" | "episodedetails" | "movie" | "musicvideo" | "artist"
    ) {
        return Err(AddonMediaNfoError::Invalid(format!(
            "unsupported NFO root {root}"
        )));
    }
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    xml(
        &mut writer,
        Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)),
    )?;
    let mut start = BytesStart::new(root);
    start.push_attribute(("xmlns:ersatzrs", "urn:ersatzrs:nfo:1"));
    xml(&mut writer, Event::Start(start))?;
    text(
        &mut writer,
        if root == "artist" { "name" } else { "title" },
        Some(&metadata.title),
    )?;
    text(
        &mut writer,
        "originaltitle",
        metadata.original_title.as_deref(),
    )?;
    text(&mut writer, "sorttitle", metadata.sort_title.as_deref())?;
    text(&mut writer, "outline", metadata.outline.as_deref())?;
    text(&mut writer, "plot", metadata.plot.as_deref())?;
    text(&mut writer, "tagline", metadata.tagline.as_deref())?;
    text(&mut writer, "showtitle", metadata.show_title.as_deref())?;
    text(&mut writer, "season", number(metadata.season).as_deref())?;
    text(&mut writer, "episode", number(metadata.episode).as_deref())?;
    text(&mut writer, "year", number(metadata.year).as_deref())?;
    text(&mut writer, "aired", metadata.release_date.as_deref())?;
    text(
        &mut writer,
        "ersatzrs:release_datetime",
        metadata.release_date.as_deref(),
    )?;
    repeated(&mut writer, "mpaa", &metadata.content_ratings)?;
    repeated(&mut writer, "genre", &metadata.genres)?;
    repeated(&mut writer, "tag", &metadata.tags)?;
    repeated(&mut writer, "studio", &metadata.studios)?;
    repeated(&mut writer, "label", &metadata.music_labels)?;
    repeated(&mut writer, "language", &metadata.languages)?;
    for person in &metadata.people {
        xml(&mut writer, Event::Start(BytesStart::new("actor")))?;
        text(&mut writer, "name", Some(&person.name))?;
        text(&mut writer, "role", person.role.as_deref())?;
        text(&mut writer, "order", number(person.order).as_deref())?;
        text(&mut writer, "thumb", person.thumb.as_deref())?;
        xml(&mut writer, Event::End(BytesEnd::new("actor")))?;
    }
    repeated(&mut writer, "credits", &metadata.writers)?;
    repeated(&mut writer, "director", &metadata.directors)?;
    repeated(&mut writer, "artist", &metadata.artists)?;
    repeated(&mut writer, "ersatzrs:producer", &metadata.producers)?;
    repeated(
        &mut writer,
        "ersatzrs:original_broadcaster",
        &metadata.original_broadcasters,
    )?;
    repeated(&mut writer, "ersatzrs:broadcaster", &metadata.broadcasters)?;
    text(
        &mut writer,
        "ersatzrs:collection",
        metadata.collection.as_deref(),
    )?;
    for artwork in metadata
        .artwork
        .iter()
        .filter(|artwork| artwork.role != AddonMediaListArtworkRole::Fanart)
    {
        let mut element = BytesStart::new("thumb");
        element.push_attribute((
            "aspect",
            match artwork.role {
                AddonMediaListArtworkRole::Poster => "poster",
                AddonMediaListArtworkRole::Fanart => "fanart",
                AddonMediaListArtworkRole::Thumb => "thumb",
                AddonMediaListArtworkRole::Other => "other",
            },
        ));
        let width = artwork.width.map(|value| value.to_string());
        let height = artwork.height.map(|value| value.to_string());
        if let Some(width) = &width {
            element.push_attribute(("width", width.as_str()));
        }
        if let Some(height) = &height {
            element.push_attribute(("height", height.as_str()));
        }
        if let Some(language) = &artwork.language {
            element.push_attribute(("language", language.as_str()));
        }
        xml(&mut writer, Event::Start(element))?;
        xml(&mut writer, Event::Text(BytesText::new(&artwork.url)))?;
        xml(&mut writer, Event::End(BytesEnd::new("thumb")))?;
    }
    let fanart = metadata
        .artwork
        .iter()
        .filter(|artwork| artwork.role == AddonMediaListArtworkRole::Fanart)
        .collect::<Vec<_>>();
    if !fanart.is_empty() {
        xml(&mut writer, Event::Start(BytesStart::new("fanart")))?;
        for artwork in fanart {
            let mut element = BytesStart::new("thumb");
            let width = artwork.width.map(|value| value.to_string());
            let height = artwork.height.map(|value| value.to_string());
            if let Some(width) = &width {
                element.push_attribute(("width", width.as_str()));
            }
            if let Some(height) = &height {
                element.push_attribute(("height", height.as_str()));
            }
            if let Some(language) = &artwork.language {
                element.push_attribute(("language", language.as_str()));
            }
            xml(&mut writer, Event::Start(element))?;
            xml(&mut writer, Event::Text(BytesText::new(&artwork.url)))?;
            xml(&mut writer, Event::End(BytesEnd::new("thumb")))?;
        }
        xml(&mut writer, Event::End(BytesEnd::new("fanart")))?;
    }
    for url in metadata.artwork_urls.iter().filter(|url| {
        metadata
            .artwork
            .iter()
            .all(|candidate| candidate.url != **url)
    }) {
        text(&mut writer, "thumb", Some(url))?;
    }
    for guid in &metadata.guids {
        let Some((source, value)) = guid.split_once("://") else {
            continue;
        };
        let mut element = BytesStart::new("uniqueid");
        element.push_attribute(("type", source));
        xml(&mut writer, Event::Start(element))?;
        xml(&mut writer, Event::Text(BytesText::new(value)))?;
        xml(&mut writer, Event::End(BytesEnd::new("uniqueid")))?;
    }
    xml(&mut writer, Event::End(BytesEnd::new(root)))?;
    let mut bytes = writer.into_inner();
    bytes.push(b'\n');
    Ok(bytes)
}

fn parse(bytes: &[u8]) -> Result<AddonMediaListNfoMetadata> {
    let input = std::str::from_utf8(bytes)
        .map_err(|error| AddonMediaNfoError::Invalid(error.to_string()))?;
    if !["tvshow", "episodedetails", "movie", "musicvideo", "artist"]
        .iter()
        .any(|root| input.contains(&format!("<{root}")))
    {
        return Err(AddonMediaNfoError::Invalid(
            "missing supported root element".to_owned(),
        ));
    }
    let raw: RawNfo = quick_xml::de::from_str(input)
        .map_err(|error| AddonMediaNfoError::Invalid(error.to_string()))?;
    let mut artwork = Vec::new();
    let mut artwork_urls = Vec::new();
    for candidate in raw.artwork {
        let Some(url) = candidate.url.and_then(|url| non_empty_owned(&url)) else {
            continue;
        };
        let role = match candidate.aspect.as_deref() {
            Some("poster") => Some(AddonMediaListArtworkRole::Poster),
            Some("fanart") => Some(AddonMediaListArtworkRole::Fanart),
            Some("thumb") => Some(AddonMediaListArtworkRole::Thumb),
            Some("other") => Some(AddonMediaListArtworkRole::Other),
            _ => None,
        };
        if let Some(role) = role {
            artwork.push(AddonMediaListArtwork {
                url,
                role,
                width: candidate.width,
                height: candidate.height,
                language: candidate.language,
            });
        } else {
            artwork_urls.push(url);
        }
    }
    for candidate in raw.fanart.into_iter().flat_map(|fanart| fanart.thumbs) {
        let Some(url) = candidate.url.and_then(|url| non_empty_owned(&url)) else {
            continue;
        };
        artwork.push(AddonMediaListArtwork {
            url,
            role: AddonMediaListArtworkRole::Fanart,
            width: candidate.width,
            height: candidate.height,
            language: candidate.language,
        });
    }
    Ok(AddonMediaListNfoMetadata {
        title: raw.title.or(raw.name).unwrap_or_default(),
        original_title: raw.originaltitle,
        sort_title: raw.sorttitle,
        outline: raw.outline,
        plot: raw.plot,
        tagline: raw.tagline,
        show_title: raw.showtitle,
        season: parse_number(raw.season),
        episode: parse_number(raw.episode),
        year: parse_number(raw.year),
        release_date: raw.release_datetime.or(raw.aired).or(raw.premiered),
        content_ratings: clean(raw.content_ratings),
        genres: clean(raw.genres),
        tags: clean(raw.tags),
        studios: clean(raw.studios),
        music_labels: clean(raw.music_labels),
        languages: clean(raw.languages),
        people: raw
            .people
            .into_iter()
            .filter_map(|person| {
                Some(AddonMediaListNfoPerson {
                    name: person.name?.trim().to_owned(),
                    role: person.role,
                    order: person.order,
                    thumb: person.thumb,
                })
            })
            .collect(),
        writers: clean(raw.writers),
        directors: clean(raw.directors),
        artists: clean(raw.artists),
        producers: clean(raw.producers),
        original_broadcasters: clean(raw.original_broadcasters),
        broadcasters: clean(raw.broadcasters),
        collection: raw.collection,
        artwork,
        artwork_urls,
        guids: raw
            .guids
            .into_iter()
            .filter_map(|guid| Some(format!("{}://{}", guid.source?, guid.value?)))
            .collect(),
    })
}

fn publish_with_state(
    path: &Path,
    bytes: &[u8],
    ownership: AddonMediaListNfoOwnership,
    error: Option<String>,
) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| AddonMediaNfoError::Write(source.to_string()))?;
    let mut state = load_state(parent)?;
    state.version = 1;
    state.files.insert(
        file_key(path)?,
        NfoFileState {
            ownership,
            sha256: hash(bytes),
            error,
        },
    );
    let previous = fs::read(path).ok();
    write_atomically(path, bytes)?;
    if let Err(state_error) = save_state(parent, &state) {
        if let Some(previous) = previous {
            let _ = write_atomically(path, &previous);
        } else {
            let _ = fs::remove_file(path);
        }
        return Err(state_error);
    }
    Ok(())
}

fn load_state(folder: &Path) -> Result<NfoState> {
    let path = folder.join(STATE_FILE);
    if !path.is_file() {
        return Ok(NfoState {
            version: 1,
            files: BTreeMap::new(),
        });
    }
    let bytes =
        fs::read(&path).map_err(|error| AddonMediaNfoError::StateRead(error.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|error| AddonMediaNfoError::StateRead(error.to_string()))
}

fn save_state(folder: &Path, state: &NfoState) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| AddonMediaNfoError::StateWrite(error.to_string()))?;
    write_atomically(&folder.join(STATE_FILE), &bytes)
        .map_err(|error| AddonMediaNfoError::StateWrite(error.to_string()))
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| AddonMediaNfoError::Write(error.to_string()))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.flush())
        .map_err(|error| AddonMediaNfoError::Write(error.to_string()))?;
    temporary
        .persist(path)
        .map_err(|error| AddonMediaNfoError::Write(error.to_string()))?;
    Ok(())
}

fn file_key(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .ok_or(AddonMediaNfoError::PathUnavailable)
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a string cannot fail");
            output
        })
}

fn clean(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect()
}

fn non_empty_owned(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}

fn parse_number(value: Option<String>) -> Option<i32> {
    value.and_then(|value| value.trim().parse().ok())
}

fn number(value: Option<i32>) -> Option<String> {
    value.map(|value| value.to_string())
}

fn repeated(writer: &mut Writer<Vec<u8>>, name: &str, values: &[String]) -> Result<()> {
    for value in values {
        text(writer, name, Some(value))?;
    }
    Ok(())
}

fn text(writer: &mut Writer<Vec<u8>>, name: &str, value: Option<&str>) -> Result<()> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    xml(writer, Event::Start(BytesStart::new(name)))?;
    xml(writer, Event::Text(BytesText::new(value)))?;
    xml(writer, Event::End(BytesEnd::new(name)))
}

fn xml(writer: &mut Writer<Vec<u8>>, event: Event<'_>) -> Result<()> {
    writer
        .write_event(event)
        .map_err(|error| AddonMediaNfoError::Write(error.to_string()))
}

/// Resolve the same-stem NFO path for a managed Remote Stream definition.
#[must_use]
pub fn item_nfo_path(definition_path: Option<&str>) -> Option<PathBuf> {
    definition_path
        .map(Path::new)
        .map(|path| path.with_extension("nfo"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_episode_metadata_round_trips() {
        let metadata = AddonMediaListNfoMetadata {
            title: "Episode".to_owned(),
            original_title: Some("Original".to_owned()),
            show_title: Some("Show".to_owned()),
            season: Some(2),
            episode: Some(3),
            genres: vec!["Archive".to_owned()],
            people: vec![AddonMediaListNfoPerson {
                name: "Person".to_owned(),
                role: Some("Presenter".to_owned()),
                ..AddonMediaListNfoPerson::default()
            }],
            artwork_urls: vec!["https://example.test/thumb.jpg".to_owned()],
            guids: vec!["fixture://episode-3".to_owned()],
            ..AddonMediaListNfoMetadata::default()
        };
        let encoded = serialize("episodedetails", &metadata).expect("serialize NFO");
        let decoded = parse(&encoded).expect("parse NFO");
        assert_eq!(decoded, metadata);
    }

    #[test]
    fn artist_parent_uses_kodi_artist_name_field() {
        let metadata = AddonMediaListNfoMetadata {
            title: "Fixture artist".to_owned(),
            plot: Some("Biography".to_owned()),
            ..AddonMediaListNfoMetadata::default()
        };
        let encoded = serialize("artist", &metadata).expect("serialize artist NFO");
        let xml = std::str::from_utf8(&encoded).expect("UTF-8 artist NFO");
        assert!(xml.contains("<artist"));
        assert!(xml.contains("<name>Fixture artist</name>"));
        assert!(!xml.contains("<title>"));
        assert_eq!(parse(&encoded).expect("parse artist NFO"), metadata);
    }

    #[test]
    fn kodi_artwork_roles_round_trip_with_nested_fanart() {
        let metadata = AddonMediaListNfoMetadata {
            title: "Episode".to_owned(),
            artwork: vec![
                AddonMediaListArtwork {
                    url: "poster.jpg".to_owned(),
                    role: AddonMediaListArtworkRole::Poster,
                    width: Some(1000),
                    height: Some(1500),
                    language: None,
                },
                AddonMediaListArtwork {
                    url: "thumb.jpg".to_owned(),
                    role: AddonMediaListArtworkRole::Thumb,
                    width: None,
                    height: None,
                    language: Some("en".to_owned()),
                },
                AddonMediaListArtwork {
                    url: "fanart.jpg".to_owned(),
                    role: AddonMediaListArtworkRole::Fanart,
                    width: Some(1920),
                    height: Some(1080),
                    language: None,
                },
            ],
            ..AddonMediaListNfoMetadata::default()
        };

        let encoded = serialize("episodedetails", &metadata).expect("serialize artwork NFO");
        let xml = std::str::from_utf8(&encoded).expect("UTF-8 artwork NFO");
        assert!(
            xml.contains(r#"<thumb aspect="poster" width="1000" height="1500">poster.jpg</thumb>"#)
        );
        assert!(xml.contains(r#"<thumb aspect="thumb" language="en">thumb.jpg</thumb>"#));
        assert!(xml.contains("<fanart>"), "{xml}");
        let fanart_block = xml
            .split_once("<fanart>")
            .and_then(|(_, remainder)| remainder.split_once("</fanart>"))
            .map(|(block, _)| block)
            .expect("serialized fanart container");
        assert!(
            fanart_block.contains(">fanart.jpg</thumb>"),
            "fanart must be nested in Kodi's fanart container: {xml}"
        );
        assert_eq!(parse(&encoded).expect("parse artwork NFO"), metadata);
    }

    #[test]
    fn external_edit_becomes_user_owned_and_survives_generation() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let path = temp.path().join("episode.nfo");
        let generated = serialize(
            "episodedetails",
            &AddonMediaListNfoMetadata {
                title: "Generated".to_owned(),
                ..AddonMediaListNfoMetadata::default()
            },
        )
        .expect("generated NFO");
        reconcile_generated(&path, &generated).expect("publish generated NFO");
        let user = serialize(
            "episodedetails",
            &AddonMediaListNfoMetadata {
                title: "User".to_owned(),
                ..AddonMediaListNfoMetadata::default()
            },
        )
        .expect("user NFO");
        fs::write(&path, &user).expect("external edit");
        let effective = reconcile_generated(&path, &generated).expect("preserve user NFO");
        assert_eq!(effective, user);
        assert_eq!(
            read_form(Some(&path)).expect("form").ownership,
            AddonMediaListNfoOwnership::User
        );
    }

    #[test]
    fn empty_untracked_placeholder_is_replaced_by_generated_metadata() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let path = temp.path().join("tvshow.nfo");
        fs::write(&path, []).expect("empty placeholder");
        let generated = serialize(
            "tvshow",
            &AddonMediaListNfoMetadata {
                title: "Provider Series".to_owned(),
                plot: Some("Provider introduction".to_owned()),
                ..AddonMediaListNfoMetadata::default()
            },
        )
        .expect("generated NFO");

        let effective = reconcile_generated(&path, &generated).expect("replace placeholder");

        assert_eq!(effective, generated);
        let form = read_form(Some(&path)).expect("generated form");
        assert_eq!(form.metadata.title, "Provider Series");
        assert_eq!(form.ownership, AddonMediaListNfoOwnership::Generated);
    }
}
