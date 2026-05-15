# Volledige ErsatzTV metadata-parity voor Show/Season/Episode (Local/Emby/Jellyfin) + Music Videos via /media/music/artists (Local)

## Context

Vandaag bleek dat `/media/tv/shows` leeg bleef ondanks een succesvolle Jellyfin Series-scan. Oorzaak: `ensure_jellyfin_show_item` schreef alleen rijen in `Show` en `JellyfinShow`, en geen `ShowMetadata` — terwijl `list_shows` joint op `ShowMetadata`. Die specifieke bug is vandaag al gefixt voor Jellyfin × Show in [crates/ersatzrs-infra/src/jellyfin.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/jellyfin.rs) (commit nog niet gepusht).

Verdere inspectie laat zien dat dit gat veel breder is dan alleen Jellyfin × Show: vrijwel alle metadata-tabellen voor non-Movie kinds blijven ongeschreven. Concreet:

| Provider × Kind | Metadata-rij geschreven? | Artwork? | Guids? | Genres/Tags/Studios/Actors/Directors/Writers? |
|---|---|---|---|---|
| Jellyfin × Show | ✅ (vandaag) | ✅ | ✅ | ❌ |
| Jellyfin × Season | ❌ | ❌ | ❌ | ❌ |
| Jellyfin × Episode | ❌ | ❌ | ❌ | ❌ |
| Emby × Show | ❌ | ❌ | ❌ | ❌ |
| Emby × Season | ❌ | ❌ | ❌ | ❌ |
| Emby × Episode | ❌ | ❌ | ❌ | ❌ |
| Local × Show/Season/Episode | ❌ (hele scanner-tak weggegooid als `UnsupportedLibraryMediaKind`) | — | — | — |
| Local × Artist | ⚠️ alleen Title + DateAdded (zie [`upsert_artist_for_folder`](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/lib.rs#L336)) | ❌ | ❌ | ❌ Genres/Styles/Moods |
| Local × MusicVideo | ⚠️ alleen Title + DateAdded (zie [`upsert_local_music_video`](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/lib.rs#L389)) | ❌ | ❌ | ❌ Genres/Tags/Studios/Directors/Artists |

Gewenste eindstaat: `/media/tv/shows` toont rijke TV-kaarten met poster/fanart/jaar/plot/genres/tags/studios/actors voor alle drie de providers; idem `/media/music/artists` toont artists met thumb/biography/genres/styles voor local libraries, en hun music-video-kinderen zijn klaar voor een toekomstige per-artist detailpagina.

Scope (door gebruiker bevestigd): **alles in één grote commit**, **volledige ETV-parity per episode** (incl. Directors/Writers/Actors/Genres/Tags/Artwork/Guids), en **Local TV-scanner from scratch** porten (incl. `tvshow.nfo` / per-episode NFO parsing).

ErsatzTV (`$ERSATZTV_SRC` = `/home/bart/ErsatzTV/`) ondersteunt **geen** music videos of artists in Jellyfin/Emby — alleen Local. Music-video-parity is dus enkel Local.

---

## Pattern (Movie als referentie, met uitbreidingen)

De huidige Movie-pad is de referentie voor het API-provider-patroon. Hergebruik de patroon-helpers en breidt uit met child-relatie-helpers waar Movie zelf nu nog niets schrijft.

**Bestaande helpers (Movie):**
- [`upsert_movie_metadata`](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/jellyfin.rs#L1804) — INSERT/UPDATE op `MovieMetadata`
- [`replace_movie_artwork`](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/jellyfin.rs#L1866) — DELETE+INSERT poster (kind 0) + fanart (kind 3)
- [`insert_movie_artwork`](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/jellyfin.rs#L1885) — single-row insert
- [`replace_movie_guids`](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/jellyfin.rs#L1908) — DELETE+INSERT in `MetadataGuid`
- [`encode_datetime`](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/jellyfin.rs#L2453) — uniforme datetime-formatter

**Schema-observatie**: `Genre`, `Tag`, `Studio`, `Actor`, `Director`, `Writer`, `Style`, `Mood`, `MusicVideoArtist`, `MetadataGuid` en `Artwork` zijn sparse "shared" tabellen met `Nullable` FK-kolommen per metadata-soort (bv. `Genre.MovieMetadataId`, `Genre.ShowMetadataId`, `Genre.EpisodeMetadataId`, `Genre.ArtistMetadataId`, `Genre.MusicVideoMetadataId`). Dat betekent: één generieke `replace_genres/tags/studios/actors/...` is mogelijk via een `kind_fk_column: &str` parameter.

**Nieuwe gedeelde helpers** in [crates/ersatzrs-infra/src/metadata.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/metadata.rs) (nieuw module, achter `pub(crate)`):
```rust
pub(crate) async fn replace_strings(
    tx, table: &'static str, fk_column: &'static str, metadata_id: i64, values: &[String]
) -> Result<()>
```
Werkt voor `Genre`, `Tag`, `Studio`, `Style`, `Mood`, `MusicVideoArtist` (allemaal: `Id PK auto`, `Name TEXT`, `{Kind}MetadataId FK`).

```rust
pub(crate) async fn replace_people(
    tx, table: &'static str, fk_column: &'static str, metadata_id: i64, people: &[PersonRef]
) -> Result<()>
```
Werkt voor `Actor`, `Director`, `Writer`. `Actor` heeft extra `Role` + `Order`-kolommen; helper accepteert `PersonRef { name, role: Option<String>, order: Option<i32> }` en negeert lege Role/Order voor Director/Writer.

```rust
pub(crate) async fn replace_artwork(
    tx, fk_column: &'static str, metadata_id: i64, items: &[ArtworkRef]
) -> Result<()>
```
DELETE `WHERE {fk_column} = ?` met `ArtworkKind IN (...)` van de meegegeven kinds, dan INSERT.

```rust
pub(crate) async fn replace_guids(
    tx, fk_column: &'static str, metadata_id: i64, guids: &[String]
) -> Result<()>
```

Bestaande movie-specifieke wrappers blijven werken (en zouden retroactief naar de generieke helpers gemigreerd kunnen worden in een vervolg-slice).

---

## Implementatie per slice

### 1. Generieke metadata-helper module
**Nieuw**: [crates/ersatzrs-infra/src/metadata.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/metadata.rs)

Bevat `replace_strings`, `replace_people`, `replace_artwork`, `replace_guids` zoals hierboven, plus de typen `PersonRef`, `ArtworkRef`, `ArtworkKind` (constants `POSTER=0, THUMBNAIL=1, LOGO=2, FANART=3` — uit ETV's `ArtworkKind` enum). Re-export via `crate::metadata::*` in [lib.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/lib.rs).

### 2. Jellyfin × Show — child-relaties toevoegen
**Bestand**: [crates/ersatzrs-infra/src/jellyfin.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/jellyfin.rs), [crates/ersatzrs-core/src/repositories/jellyfin.rs](/home/bart/ErsatzRS/crates/ersatzrs-core/src/repositories/jellyfin.rs)

- Uitbreiden `JellyfinShowItem` (al uitgebreid vandaag voor metadatakern) met `genres: Vec<String>`, `tags: Vec<String>`, `studios: Vec<String>`, `actors: Vec<ActorRef>`, `thumb_artwork_path: Option<String>` (ArtworkKind 1).
- `project_show` vult deze velden uit `item.genres`, `item.tags`, `item.studios`, `item.people` (filter op `Type=="Actor"`, sla `Role` + index op).
- `fetch_show_page` voegt `People,ImageTags` toe aan de `fields` query (al deels gedaan vandaag).
- `upsert_show_metadata` roept aanvullend `replace_strings(tx, "Genre", "ShowMetadataId", id, genres)`, idem `Tag`/`Studio`/`Actor` (via `replace_people`).
- `replace_show_artwork` accepteert 3 kinds (Poster, Thumbnail, FanArt) i.p.v. 2.

### 3. Jellyfin × Season — volledig schrijven
**Bestand**: zelfde

- Uitbreiden `JellyfinSeasonItem`: `sort_title`, `year`, `date_created`, `release_date`, `poster_artwork_path`, `fanart_artwork_path`, `provider_guids`.
- `fetch_season_page` voegt `DateCreated,Etag,ProviderIds,PremiereDate` toe aan fields (huidige fetch heeft alleen `Etag`).
- `project_season` projecteert volledig.
- Nieuwe `upsert_season_metadata(tx, season_id, item)` + `replace_season_artwork` + `replace_season_guids`. (SeasonMetadata-schema bevat `Title, SortTitle, Year, ReleaseDate, DateAdded, DateUpdated, MetadataKind`. Geen `Plot/Tagline/ContentRating`.)
- Aanroep toevoegen in `upsert_jellyfin_season`.

### 4. Jellyfin × Episode — volledig schrijven
**Bestand**: zelfde

- `JellyfinMediaItem` wordt al gebruikt voor episodes (gedeeld met movies). Voeg `episode_number: Option<i32>`, `season_number: Option<i32>`, `genres/tags/actors/directors/writers` toe.
- `project_episode` (nieuw — niet langer `project_movie` recyclen) projecteert volledig en gebruikt `item.index_number` voor episode_number.
- `fetch_episode_page` (regel ~178) krijgt `People` extra in fields.
- Nieuwe `upsert_episode_metadata` + `replace_episode_artwork` (kind 1 Thumbnail) + `replace_episode_guids` + Genres/Tags/Actors/Directors/Writers via gedeelde helpers.
- Aanroep toevoegen in [`upsert_jellyfin_episode`](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/jellyfin.rs#L1357).

### 5. Emby × Show / Season / Episode — symmetrische port
**Bestand**: [crates/ersatzrs-infra/src/emby.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/emby.rs), [crates/ersatzrs-core/src/repositories/emby.rs](/home/bart/ErsatzRS/crates/ersatzrs-core/src/repositories/emby.rs)

- `EmbyShowItem`/`EmbySeasonItem`/`EmbyMediaItem` krijgen dezelfde velden als de Jellyfin-equivalenten.
- `project_show` (regel 962, momenteel maar 3 velden), `project_season`, `project_episode` worden volledig uitgewerkt.
- `fetch_show_page`/`fetch_season_page`/`fetch_episode_page` krijgen de uitgebreide `fields=` parameter.
- `upsert_emby_show`/`upsert_emby_season`/`upsert_emby_episode` roepen nieuwe `upsert_*_metadata` helpers (gedupliceerd uit jellyfin.rs, of beter: gefactoreerd naar het nieuwe `metadata.rs` als `upsert_show_metadata_generic(..., item: impl ShowMetadataInput)` — beslissing tijdens uitvoering).
- Artwork-pad-formaat is `emby://Items/{id}/Images/Primary?tag={tag}` i.p.v. `jellyfin://...`.

### 6. Local × Show / Season / Episode — nieuwe scanner-tak

**Bestand**: [crates/ersatzrs-scanner/src/lib.rs](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/lib.rs) (verwijder de `UnsupportedLibraryMediaKind` returns voor `Shows`), nieuw [crates/ersatzrs-scanner/src/local_television.rs](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/local_television.rs), nieuw [crates/ersatzrs-scanner/src/nfo.rs](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/nfo.rs)

Port van [ErsatzTV.Scanner/Core/Metadata/TelevisionFolderScanner.cs](/home/bart/ErsatzTV/ErsatzTV.Scanner/Core/Metadata/TelevisionFolderScanner.cs) (648 regels). Pseudocode:
```
for each show_folder under library root:
    parse tvshow.nfo (or fallback: folder name) → ShowMetadata
    upsert Show + ShowMetadata + Artwork (poster.*, folder.*, fanart.*, thumb.*) + Guids + Genres/Tags/Studios/Actors

    for each season_folder under show_folder:
        parse "Season XX" from folder name → SeasonNumber
        upsert Season + SeasonMetadata (Title="Season XX", SortTitle) + Artwork (poster.*, fanart.*)

        for each video_file under season_folder:
            parse {basename}.nfo → EpisodeMetadata (incl Directors/Writers/Actors)
            ffprobe → MediaVersion + MediaFile + streams + chapters
            upsert Episode + EpisodeMetadata + Artwork ({basename}-thumb.*) + Guids + Genres/Tags/Actors/Directors/Writers
```

**Nieuwe afhankelijkheid**: voeg `quick-xml = { version = "0.x", features = ["serialize"] }` toe in [crates/ersatzrs-scanner/Cargo.toml](/home/bart/ErsatzRS/crates/ersatzrs-scanner/Cargo.toml). Reeds transitief beschikbaar in `Cargo.lock`, dus geen extra build-overhead.

**NFO-parser** (`nfo.rs`):
- `parse_tvshow_nfo(path) -> Result<TvShowNfo>` met velden zoals `title, sorttitle, plot, year, premiered, contentrating, tagline, genres, tags, studios, actors (List<{name, role, order}>), guids (List<{kind, value}>)`. Kodi/Plex-XML-schema, root element `<tvshow>`.
- `parse_episode_nfo(path) -> Result<EpisodeNfo>` met `title, plot, premiered, year, episode, season, genres, tags, directors, writers, actors, guids`. Root: `<episodedetails>`.
- `parse_season_nfo(path) -> Result<SeasonNfo>` (zelden bestaand; fallback OK).
- Met `quick-xml::de::from_reader` + `serde::Deserialize`-structs.

**SortTitle-helper** (port van [`SortTitle.GetSortTitle`](/home/bart/ErsatzTV/ErsatzTV.Core/Metadata/SortTitle.cs)): strip leading "The/A/An ", lowercase, trim. Toevoegen in `ersatzrs-core` of `ersatzrs-infra`.

**Folder-walking**: `walkdir` is al in workspace dependencies.

**Artwork-discovery** (port van `LocalFolderScanner.LocateArtwork`):
- Show: zoek `poster.{jpg,jpeg,png,tbn}` of `folder.{...}` voor Poster (kind 0); `fanart.{...}` of `backdrop.{...}` voor FanArt (kind 3); `thumb.{...}` voor Thumbnail (kind 1).
- Season: `poster.{...}` / `season01-poster.{...}` patroon.
- Episode: `{basename}-thumb.{...}` of `{basename}.tbn`.
- Sla het lokale bestandspad op in `Artwork.Path` (absolute path, geen URL).

### 7. Local × Artist — metadata-enrichment

**Bestand**: [crates/ersatzrs-scanner/src/lib.rs](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/lib.rs) (functie `upsert_artist_for_folder` rond regel 336)

Port van [LocalMetadataProvider.GetMetadataForArtist](/home/bart/ErsatzTV/ErsatzTV.Scanner/Core/Metadata/LocalMetadataProvider.cs#L101). Stappen:
- Voor elke artist-folder: probeer `{folder}/artist.nfo` (nieuwe `parse_artist_nfo` in `nfo.rs`); bij falen vallen we terug op alleen folder name.
- Vul `ArtistMetadata` volledig: `Title`, `SortTitle` (via SortTitle-helper), `Disambiguation`, `Biography`, `Year`, `Formed`, `DateAdded`, `DateUpdated`, `MetadataKind=1` (Sidecar als NFO aanwezig, anders Fallback=0).
- Schrijf child-tabellen: `Genres`, `Styles`, `Moods` via `replace_strings` per `ArtistMetadataId`.
- Artwork-discovery in artist-folder: `thumb.{...}` of `profile.{...}` → kind 1 Thumbnail; `fanart.{...}` → kind 3 FanArt; `logo.{...}` → kind 2 Logo.

### 8. Local × MusicVideo — metadata-enrichment

**Bestand**: [crates/ersatzrs-scanner/src/lib.rs](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/lib.rs) (functie `upsert_local_music_video` rond regel 389)

Port van [LocalMetadataProvider.LoadMusicVideoMetadata](/home/bart/ErsatzTV/ErsatzTV.Scanner/Core/Metadata/LocalMetadataProvider.cs#L304).
- Probeer `{video}.nfo` zelfde basename als videobestand (`parse_music_video_nfo`): Root `<musicvideo>`, velden `title, album, plot, year, premiered, track, artists (list), directors, genres, tags, studios`.
- `MusicVideoMetadata` volledig vullen.
- Child-tabellen: `Genres`, `Tags`, `Studios` → `replace_strings`; `Directors` → `replace_people`; `Artists` → `replace_strings` op tabel `MusicVideoArtist`.
- Artwork-discovery: `poster.{...}` / `{basename}-poster.{...}` → kind 0; `{basename}-thumb.{...}` of `thumb.{...}` → kind 1; `fanart.{...}` → kind 3.

### 9. Tests
**Bestanden**:
- [crates/ersatzrs-infra/tests/jellyfin_service.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/tests/jellyfin_service.rs) — count-assertions voor `ShowMetadata`, `SeasonMetadata`, `EpisodeMetadata`, plus `Genre`, `Tag`, `Studio`, `Actor`, `Director`, `Writer`, `Artwork`, `MetadataGuid` per kind ≥ 1 na een show-sync.
- [crates/ersatzrs-infra/tests/emby_service.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/tests/emby_service.rs) — idem, plus mock-responses uitbreiden met dezelfde rijke metadata-velden.
- Nieuw `crates/ersatzrs-scanner/tests/local_television.rs` — een tijdelijke folderstructuur (via `tempfile`) met `tvshow.nfo`, één season-folder, één video + NFO; verifieer dat alle metadata in DB komt.
- Nieuw `crates/ersatzrs-scanner/tests/local_music_video.rs` — idem voor music video + artist.
- Nieuw `crates/ersatzrs-scanner/tests/nfo.rs` — pure unit-tests voor de XML-parsers met fixture-strings.

Insta-snapshots bijwerken waar van toepassing (de bestaande Jellyfin/Emby snapshots tonen alleen episode-projecties; toevoegen van breder `ShowMetadata`/`SeasonMetadata` row-projecties is optioneel maar nuttig voor regressie-bewaking).

### 10. UI-impact (geen wijzigingen vereist)
De bestaande `/media/tv/shows` (`list_shows` in [crates/ersatzrs-web/src/admin/server_fns/media_browser.rs](/home/bart/ErsatzRS/crates/ersatzrs-web/src/admin/server_fns/media_browser.rs)) joint al op `ShowMetadata` + `Artwork` kind 0; werkt zonder wijziging zodra de scanners metadata schrijven. Idem `list_artists`. Een toekomstige `/media/tv/shows/:id` (season/episode-list) is geen onderdeel van dit plan maar profiteert dan automatisch van de rijke episode-data.

### 11. Documentatie
- [STATUS.md](/home/bart/ErsatzRS/STATUS.md): voeg slice-regel toe in het session log + checklist-update.
- [docs/reference/parity-matrix.md](/home/bart/ErsatzRS/docs/reference/parity-matrix.md): markeer Show/Season/Episode metadata-port als compleet voor Local/Emby/Jellyfin, en Music Video / Artist metadata voor Local.

---

## Kritieke bestanden om aan te raken

| Pad | Wijziging |
|---|---|
| [crates/ersatzrs-core/src/repositories/jellyfin.rs](/home/bart/ErsatzRS/crates/ersatzrs-core/src/repositories/jellyfin.rs) | `JellyfinShowItem` (al uitgebreid vandaag), `JellyfinSeasonItem`, `JellyfinMediaItem` uitbreiden |
| [crates/ersatzrs-core/src/repositories/emby.rs](/home/bart/ErsatzRS/crates/ersatzrs-core/src/repositories/emby.rs) | `EmbyShowItem`, `EmbySeasonItem`, `EmbyMediaItem` uitbreiden |
| [crates/ersatzrs-infra/src/jellyfin.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/jellyfin.rs) | `project_show`/`project_season`/`project_episode` uitbreiden; `fetch_*_page` fields-strings; nieuwe `upsert_season_metadata`/`upsert_episode_metadata` + child-relatie writes |
| [crates/ersatzrs-infra/src/emby.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/emby.rs) | Symmetrisch aan jellyfin.rs |
| [crates/ersatzrs-infra/src/metadata.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/metadata.rs) | **Nieuw** — gedeelde `replace_strings`/`replace_people`/`replace_artwork`/`replace_guids` |
| [crates/ersatzrs-infra/src/lib.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/lib.rs) | `pub mod metadata` |
| [crates/ersatzrs-scanner/src/lib.rs](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/lib.rs) | Verwijder `UnsupportedLibraryMediaKind` voor Shows; dispatch naar `local_television::sync_local_show_library`; uitbreiden `upsert_artist_for_folder` + `upsert_local_music_video` met NFO + artwork + child-relaties |
| [crates/ersatzrs-scanner/src/local_television.rs](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/local_television.rs) | **Nieuw** — port van `TelevisionFolderScanner.cs` |
| [crates/ersatzrs-scanner/src/nfo.rs](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/nfo.rs) | **Nieuw** — `quick-xml`/`serde`-based parsers voor `tvshow.nfo` / episode-NFO / `artist.nfo` / music-video-NFO; `SortTitle::get` |
| [crates/ersatzrs-scanner/Cargo.toml](/home/bart/ErsatzRS/crates/ersatzrs-scanner/Cargo.toml) | `quick-xml = { workspace = true }` (en in root `Cargo.toml` toevoegen aan `[workspace.dependencies]`) |
| [crates/ersatzrs-infra/tests/jellyfin_service.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/tests/jellyfin_service.rs) | Mock-response verrijken; assert child-rows |
| [crates/ersatzrs-infra/tests/emby_service.rs](/home/bart/ErsatzRS/crates/ersatzrs-infra/tests/emby_service.rs) | Idem |
| `crates/ersatzrs-scanner/tests/local_television.rs` | **Nieuw** |
| `crates/ersatzrs-scanner/tests/local_music_video.rs` | **Nieuw** |
| `crates/ersatzrs-scanner/tests/nfo.rs` | **Nieuw** |
| [STATUS.md](/home/bart/ErsatzRS/STATUS.md) | Session log + checklist |
| [docs/reference/parity-matrix.md](/home/bart/ErsatzRS/docs/reference/parity-matrix.md) | Parity-status update |

---

## Volgorde van uitvoering (één commit, intern logisch geordend)

1. [x] Voeg `quick-xml` workspace-dep + `Cargo.toml` updates toe.
2. [x] Maak [`metadata.rs`](/home/bart/ErsatzRS/crates/ersatzrs-infra/src/metadata.rs) module met gedeelde helpers (incl. `ArtworkKind` constants).
3. [x] Migreer Movie-helpers naar de generieke pad (movie-paden blijven werken via dunne wrappers).
4. [x] Verrijk Jellyfin × Show met child-relaties (Genres/Tags/Studios/Actors/Thumbnail).
5. [x] Implementeer Jellyfin × Season + Episode volledig.
6. [x] Implementeer Emby × Show + Season + Episode volledig.
7. [x] Maak [`nfo.rs`](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/nfo.rs) met serde-based parsers + SortTitle.
8. [x] Maak [`local_television.rs`](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/local_television.rs) (TV-folder-scanner).
9. [x] Wire Local × Show in [`lib.rs`](/home/bart/ErsatzRS/crates/ersatzrs-scanner/src/lib.rs) (verwijder de `UnsupportedLibraryMediaKind` returns).
10. [x] Verrijk `upsert_artist_for_folder` met NFO + artwork + Genres/Styles/Moods.
11. [x] Verrijk `upsert_local_music_video` met NFO + artwork + child-relaties.
12. [x] Tests (zowel uitgebreid bestaand als nieuw).
13. [x] STATUS + parity-matrix docs.
14. [x] `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`, `cargo insta review` waar nodig.

Verwachte commit-grootte: zeer fors (vermoedelijk 4.000–6.000 regels gewijzigd over ~15 bestanden). De gebruiker heeft expliciet voor één commit gekozen; geen splitsing.

---

## Verificatie (end-to-end)

1. **Build & lint**: `cargo fmt --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace --all-features` moet alles groen.
2. **Unit-tests NFO**: `cargo test -p ersatzrs-scanner nfo` — XML-parsers projecteren correct.
3. **Integratie Local TV**: `cargo test -p ersatzrs-scanner local_television` — `tempdir` met `Show/tvshow.nfo`, `Show/Season 01/Episode.mkv`+NFO; verifieer rijen in `Show`, `ShowMetadata`, `Season`, `SeasonMetadata`, `Episode`, `EpisodeMetadata`, `Artwork`, `MetadataGuid`, `Genre`, `Tag`, `Actor`, `Director`, `Writer`.
4. **Integratie Local Music Video**: `cargo test -p ersatzrs-scanner local_music_video` — `tempdir` met `Artist/artist.nfo`, `Artist/MV.mkv` + `MV.nfo`; verifieer `ArtistMetadata`, `MusicVideoMetadata`, child-rijen, `Artwork`.
5. **Jellyfin sync regressie**: `cargo test -p ersatzrs-infra --test jellyfin_service` — alle bestaande tests blijven groen; nieuwe assertions slagen.
6. **Emby sync regressie**: idem voor `emby_service`.
7. **Handmatig (gebruiker)**:
   - Verwijder bestaande SQLite DB of voer een fresh scan uit van Jellyfin Series-library; `/media/tv/shows` toont rijke kaarten met poster/year.
   - Trigger Emby Series-scan; idem.
   - Scan een lokale TV-folder (eerder genegeerd); shows verschijnen op `/media/tv/shows`.
   - Scan een lokale music-folder met `artist.nfo`; `/media/music/artists` toont artist-thumb + disambiguation.
8. **Snapshot-review**: `cargo insta review` voor eventueel uitgebreide projection-snapshots.

## Risico's & mitigaties

- **NFO XML-edge cases**: Kodi/Plex-NFO's variëren (legacy `<thumb>` vs `<art>`, multi-value vs comma-separated). Mitigatie: tolerant parsen (optionele velden, `#[serde(default)]`), niet falen op missende velden — bij parse-fout fallback op folder-naam-metadata (zoals ETV).
- **Local TV-scanner is een grote port**: ~648 regels C# + 2 NFO-readers. Risico op subtiele afwijking van ETV-gedrag (bv. seasonless episodes in show-root, "Specials"-folder als Season 0). Mitigatie: spiegel ETV's `TelevisionFolderScanner.cs` 1-op-1 inclusief edge-cases; review afzonderlijk.
- **Eén grote commit is review-onvriendelijk**: door de gebruiker geaccepteerd. Mitigatie: zeer beschrijvende commit message met genummerde sectie per slice, en duidelijke per-bestand-impact in PR-body indien later gepusht.
- **Bestaande DB-rijen zonder metadata**: gebruikers met al gescande show-libraries moeten herscannen. De upsert-functies detecteren bestaande items en INSERTen alleen ontbrekende `*Metadata`-rijen — bestaande data blijft.
- **Movie-pad blijft achter**: dit plan brengt Show/Episode/Artist/MusicVideo naar volledige ETV-parity, maar Movie schrijft nog steeds geen Genres/Tags/Studios/Actors/Directors/Writers. Mitigatie: de gedeelde `metadata.rs` helpers zijn klaar voor een vervolg-slice die ook Movie retroactief verrijkt; documenteer dit gat in STATUS.
